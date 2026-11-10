//! Small drawings in pixels, using kitty's graphics protocol, which kitty and Ghostty both have. An
//! image is sent ahead of time, and placeholder characters say where it goes. As they're text, a
//! window can be drawn over them.

use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use flate2::Compression;
use flate2::write::ZlibEncoder;
use ratatui::crossterm::terminal;
use ratatui::style::Color;
use vt_engine::dsp::compressor::curve_db;
use vt_engine::dsp::eq::response_db as eq_response_db;
use vt_engine::dsp::filter::track_response_db;
use vt_engine::engine::PLAYHEADS;
use vt_model::bank::{BankEntry, ContentHash, Points};
use vt_model::constants::TRACK_ENVELOPES;
use vt_model::envelope::Envelope;
use vt_model::insert::{Compressor, Equaliser, INSERTS, Insert, MAX_PARAMS};
use vt_model::sample::SampleData;
use vt_model::song::{FilterMode, FilterType, Machine, Track};

use crate::app::App;
use crate::input::mode::View;
use crate::input::picker::Shown;
use crate::sample_page::SamplePage;
use crate::sample_wave::Drawing;
use crate::theme;
use crate::track_page::{Control, Page};
use crate::views::curve::{envelope_columns, line_columns};
use crate::views::meter::{SCOPE_HEIGHT, SCOPE_WIDTH};
use crate::views::picker::{
    PREVIEW_ROWS, PREVIEW_SAMPLE_HEIGHT, PREVIEW_SAMPLE_WIDTH, PREVIEW_WAVE_HEIGHT,
    PREVIEW_WAVE_WIDTH,
};
use crate::views::sample::{
    MarkKind, WAVE_ROWS, WaveOnScreen, lines_placed, mark_reach, marks_placed, overlaid,
};
use crate::views::track::{
    DIAGRAM_HEIGHT, DIAGRAM_WIDTH, DYNAMICS_CURVE_WIDTH, EQ_CURVE_WIDTH, FILTER_CURVE_HEIGHT,
    FILTER_CURVE_WIDTH, SLOT_DRAWING_HEIGHT, WAVEFORM_HEIGHT, WAVEFORM_WIDTH,
    envelope_being_edited, filter_being_edited, slot_being_edited,
};
use crate::waveform;

/// The character that stands for a piece of an image.
pub(crate) const PLACEHOLDER: char = '\u{10EEEE}';
/// The most characters an image can cover across or down: one for each mark that numbers a row or a
/// column.
pub const MOST_CELLS: usize = 297;
/// The marks that give a placeholder's row and column, all of kitty's `rowcolumn-diacritics.txt`,
/// which Ghostty's `graphics_unicode.zig` has in the same order.
pub(crate) const POSITIONS: [char; MOST_CELLS] = [
    '\u{305}',
    '\u{30D}',
    '\u{30E}',
    '\u{310}',
    '\u{312}',
    '\u{33D}',
    '\u{33E}',
    '\u{33F}',
    '\u{346}',
    '\u{34A}',
    '\u{34B}',
    '\u{34C}',
    '\u{350}',
    '\u{351}',
    '\u{352}',
    '\u{357}',
    '\u{35B}',
    '\u{363}',
    '\u{364}',
    '\u{365}',
    '\u{366}',
    '\u{367}',
    '\u{368}',
    '\u{369}',
    '\u{36A}',
    '\u{36B}',
    '\u{36C}',
    '\u{36D}',
    '\u{36E}',
    '\u{36F}',
    '\u{483}',
    '\u{484}',
    '\u{485}',
    '\u{486}',
    '\u{487}',
    '\u{592}',
    '\u{593}',
    '\u{594}',
    '\u{595}',
    '\u{597}',
    '\u{598}',
    '\u{599}',
    '\u{59C}',
    '\u{59D}',
    '\u{59E}',
    '\u{59F}',
    '\u{5A0}',
    '\u{5A1}',
    '\u{5A8}',
    '\u{5A9}',
    '\u{5AB}',
    '\u{5AC}',
    '\u{5AF}',
    '\u{5C4}',
    '\u{610}',
    '\u{611}',
    '\u{612}',
    '\u{613}',
    '\u{614}',
    '\u{615}',
    '\u{616}',
    '\u{617}',
    '\u{657}',
    '\u{658}',
    '\u{659}',
    '\u{65A}',
    '\u{65B}',
    '\u{65D}',
    '\u{65E}',
    '\u{6D6}',
    '\u{6D7}',
    '\u{6D8}',
    '\u{6D9}',
    '\u{6DA}',
    '\u{6DB}',
    '\u{6DC}',
    '\u{6DF}',
    '\u{6E0}',
    '\u{6E1}',
    '\u{6E2}',
    '\u{6E4}',
    '\u{6E7}',
    '\u{6E8}',
    '\u{6EB}',
    '\u{6EC}',
    '\u{730}',
    '\u{732}',
    '\u{733}',
    '\u{735}',
    '\u{736}',
    '\u{73A}',
    '\u{73D}',
    '\u{73F}',
    '\u{740}',
    '\u{741}',
    '\u{743}',
    '\u{745}',
    '\u{747}',
    '\u{749}',
    '\u{74A}',
    '\u{7EB}',
    '\u{7EC}',
    '\u{7ED}',
    '\u{7EE}',
    '\u{7EF}',
    '\u{7F0}',
    '\u{7F1}',
    '\u{7F3}',
    '\u{816}',
    '\u{817}',
    '\u{818}',
    '\u{819}',
    '\u{81B}',
    '\u{81C}',
    '\u{81D}',
    '\u{81E}',
    '\u{81F}',
    '\u{820}',
    '\u{821}',
    '\u{822}',
    '\u{823}',
    '\u{825}',
    '\u{826}',
    '\u{827}',
    '\u{829}',
    '\u{82A}',
    '\u{82B}',
    '\u{82C}',
    '\u{82D}',
    '\u{951}',
    '\u{953}',
    '\u{954}',
    '\u{F82}',
    '\u{F83}',
    '\u{F86}',
    '\u{F87}',
    '\u{135D}',
    '\u{135E}',
    '\u{135F}',
    '\u{17DD}',
    '\u{193A}',
    '\u{1A17}',
    '\u{1A75}',
    '\u{1A76}',
    '\u{1A77}',
    '\u{1A78}',
    '\u{1A79}',
    '\u{1A7A}',
    '\u{1A7B}',
    '\u{1A7C}',
    '\u{1B6B}',
    '\u{1B6D}',
    '\u{1B6E}',
    '\u{1B6F}',
    '\u{1B70}',
    '\u{1B71}',
    '\u{1B72}',
    '\u{1B73}',
    '\u{1CD0}',
    '\u{1CD1}',
    '\u{1CD2}',
    '\u{1CDA}',
    '\u{1CDB}',
    '\u{1CE0}',
    '\u{1DC0}',
    '\u{1DC1}',
    '\u{1DC3}',
    '\u{1DC4}',
    '\u{1DC5}',
    '\u{1DC6}',
    '\u{1DC7}',
    '\u{1DC8}',
    '\u{1DC9}',
    '\u{1DCB}',
    '\u{1DCC}',
    '\u{1DD1}',
    '\u{1DD2}',
    '\u{1DD3}',
    '\u{1DD4}',
    '\u{1DD5}',
    '\u{1DD6}',
    '\u{1DD7}',
    '\u{1DD8}',
    '\u{1DD9}',
    '\u{1DDA}',
    '\u{1DDB}',
    '\u{1DDC}',
    '\u{1DDD}',
    '\u{1DDE}',
    '\u{1DDF}',
    '\u{1DE0}',
    '\u{1DE1}',
    '\u{1DE2}',
    '\u{1DE3}',
    '\u{1DE4}',
    '\u{1DE5}',
    '\u{1DE6}',
    '\u{1DFE}',
    '\u{20D0}',
    '\u{20D1}',
    '\u{20D4}',
    '\u{20D5}',
    '\u{20D6}',
    '\u{20D7}',
    '\u{20DB}',
    '\u{20DC}',
    '\u{20E1}',
    '\u{20E7}',
    '\u{20E9}',
    '\u{20F0}',
    '\u{2CEF}',
    '\u{2CF0}',
    '\u{2CF1}',
    '\u{2DE0}',
    '\u{2DE1}',
    '\u{2DE2}',
    '\u{2DE3}',
    '\u{2DE4}',
    '\u{2DE5}',
    '\u{2DE6}',
    '\u{2DE7}',
    '\u{2DE8}',
    '\u{2DE9}',
    '\u{2DEA}',
    '\u{2DEB}',
    '\u{2DEC}',
    '\u{2DED}',
    '\u{2DEE}',
    '\u{2DEF}',
    '\u{2DF0}',
    '\u{2DF1}',
    '\u{2DF2}',
    '\u{2DF3}',
    '\u{2DF4}',
    '\u{2DF5}',
    '\u{2DF6}',
    '\u{2DF7}',
    '\u{2DF8}',
    '\u{2DF9}',
    '\u{2DFA}',
    '\u{2DFB}',
    '\u{2DFC}',
    '\u{2DFD}',
    '\u{2DFE}',
    '\u{2DFF}',
    '\u{A66F}',
    '\u{A67C}',
    '\u{A67D}',
    '\u{A6F0}',
    '\u{A6F1}',
    '\u{A8E0}',
    '\u{A8E1}',
    '\u{A8E2}',
    '\u{A8E3}',
    '\u{A8E4}',
    '\u{A8E5}',
    '\u{A8E6}',
    '\u{A8E7}',
    '\u{A8E8}',
    '\u{A8E9}',
    '\u{A8EA}',
    '\u{A8EB}',
    '\u{A8EC}',
    '\u{A8ED}',
    '\u{A8EE}',
    '\u{A8EF}',
    '\u{A8F0}',
    '\u{A8F1}',
    '\u{AAB0}',
    '\u{AAB2}',
    '\u{AAB3}',
    '\u{AAB7}',
    '\u{AAB8}',
    '\u{AABE}',
    '\u{AABF}',
    '\u{AAC1}',
    '\u{FE20}',
    '\u{FE21}',
    '\u{FE22}',
    '\u{FE23}',
    '\u{FE24}',
    '\u{FE25}',
    '\u{FE26}',
    '\u{10A0F}',
    '\u{10A38}',
    '\u{1D185}',
    '\u{1D186}',
    '\u{1D187}',
    '\u{1D188}',
    '\u{1D189}',
    '\u{1D1AA}',
    '\u{1D1AB}',
    '\u{1D1AC}',
    '\u{1D1AD}',
    '\u{1D242}',
    '\u{1D243}',
    '\u{1D244}',
];
/// The scope is brought up to date once in this many frames, about 15 times a second. Each new
/// picture is a new image, which costs the terminal more than the program: at every frame, kitty
/// took 4.5% of a core to show it, and at every other frame, under 3%.
const SCOPE_FRAMES: usize = 2;
/// The most base64 the protocol takes in one command.
const CHUNK: usize = 4096;
/// The grid of dots an envelope is drawn on, for each character.
const CELL_DOTS_ACROSS: usize = 6;
const CELL_DOTS_DOWN: usize = 12;
/// The pixels across and down each dot. Kitty smooths an image it scales, so a dot is a solid block
/// that's already near its final size, which keeps its edges crisp.
const DOT_PIXELS: usize = 3;

/// What a filter curve's height covers, in dB, with the frequency across it from 20 Hz to 20 kHz. It
/// has more room below than above, as a filter takes away more than it adds.
const CURVE_BOTTOM_DB: f32 = -36.0;
const CURVE_TOP_DB: f32 = 24.0;

/// How far down a compressor's curve goes, in dB, both in and out, from `0` at the top and right.
/// It's as far down as the threshold goes.
const DYNAMICS_RANGE_DB: f32 = 60.0;

/// How far up and down an EQ's curve goes, in dB.
const EQ_RANGE_DB: f32 = 18.0;

/// Lines of placeholders for an image of this size, up to [`MOST_CELLS`] across and down.
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
fn png(dots: &[Vec<bool>], color: [u8; 3]) -> Vec<u8> {
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
        assert!(self.colors.len() < 16, "a picture has a few colours");
        let depth = match self.colors.len() {
            1 => 1,
            2..=3 => 2,
            _ => 4,
        };
        let per_byte = 8 / depth;
        let mut lines = Vec::with_capacity((self.width.div_ceil(per_byte) + 1) * self.height);
        for row in self.pixels.chunks(self.width) {
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
        let data = if lines.len() <= STORED_BLOCK {
            stored(&lines)
        } else {
            let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(&lines).expect("writing to memory");
            encoder.finish().expect("writing to memory")
        };

        let mut header = Vec::new();
        header.extend((self.width as u32).to_be_bytes());
        header.extend((self.height as u32).to_be_bytes());
        // Colours from a palette, and the usual settings for the rest.
        header.extend([depth as u8, 3, 0, 0, 0]);
        let mut palette = vec![0; 3];
        palette.extend(self.colors.iter().flatten());
        let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
        let chunks: [(&[u8; 4], &[u8]); 5] = [
            (b"IHDR", &header),
            (b"PLTE", &palette),
            // The first colour is see-through.
            (b"tRNS", &[0]),
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

/// An envelope's shape as a PNG: a plain line of solid dots, as on a small LCD.
pub fn envelope_png(envelope: &Envelope, color: [u8; 3], columns: usize, rows: usize) -> Vec<u8> {
    let (across, down) = (columns * CELL_DOTS_ACROSS, rows * CELL_DOTS_DOWN);
    line_png(&envelope_columns(envelope, across, down), down, color)
}

/// A track's filter and utility filter as a PNG, showing what they let through at each frequency.
pub fn filter_png(
    track: &Track,
    rate: u32,
    color: [u8; 3],
    columns: usize,
    rows: usize,
) -> Vec<u8> {
    let level = |x: f32| {
        let db = track_response_db(track, rate, 20.0 * 1000_f32.powf(x));
        (db - CURVE_BOTTOM_DB) / (CURVE_TOP_DB - CURVE_BOTTOM_DB)
    };
    let (across, down) = (columns * CELL_DOTS_ACROSS, rows * CELL_DOTS_DOWN);
    line_png(&line_columns(level, level(0.0), across, down), down, color)
}

/// A compressor's static curve as a PNG: how loud a steady level comes out, against how loud it
/// went in, each from -60 dB to 0, as the compressor works it out.
pub fn compressor_png(
    settings: Compressor,
    color: [u8; 3],
    columns: usize,
    rows: usize,
) -> Vec<u8> {
    let level = |x: f32| {
        let out = curve_db(DYNAMICS_RANGE_DB * (x - 1.0), settings);
        out / DYNAMICS_RANGE_DB + 1.0
    };
    let (across, down) = (columns * CELL_DOTS_ACROSS, rows * CELL_DOTS_DOWN);
    line_png(&line_columns(level, level(0.0), across, down), down, color)
}

/// An EQ's curve as a PNG: what it does at each frequency, from 20 Hz to 20 kHz across it, as the
/// filter's curve is drawn, and from -18 dB to +18 dB up it, a little more than a band can do.
pub fn eq_png(
    settings: Equaliser,
    rate: u32,
    color: [u8; 3],
    columns: usize,
    rows: usize,
) -> Vec<u8> {
    let level = |x: f32| {
        eq_response_db(settings, rate, 20.0 * 1000_f32.powf(x)) / (2.0 * EQ_RANGE_DB) + 0.5
    };
    let (across, down) = (columns * CELL_DOTS_ACROSS, rows * CELL_DOTS_DOWN);
    line_png(&line_columns(level, level(0.0), across, down), down, color)
}

/// The scope's picture as a PNG, from the rows it covers in each column of dots.
pub fn scope_png(spans: &[(usize, usize)], color: [u8; 3]) -> Vec<u8> {
    line_png(spans, SCOPE_HEIGHT * CELL_DOTS_DOWN, color)
}

/// A sample's waveform as a PNG, at its real level.
pub fn waveform_png(frames: &[[f32; 2]], color: [u8; 3], columns: usize, rows: usize) -> Vec<u8> {
    let (across, down) = (columns * CELL_DOTS_ACROSS, rows * CELL_DOTS_DOWN);
    line_png(&waveform::spans(frames, &[], across, down), down, color)
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

/// Write one command.
fn command(out: &mut impl Write, controls: &str, data: &[u8]) -> io::Result<()> {
    out.write_all(b"\x1b_G")?;
    out.write_all(controls.as_bytes())?;
    out.write_all(b";")?;
    out.write_all(data)?;
    out.write_all(b"\x1b\\")
}

/// A PNG to send, and how many characters it covers.
pub struct Image<'a> {
    pub id: u32,
    pub png: &'a [u8],
    pub columns: usize,
    pub rows: usize,
}

/// Send an image, in pieces. `q=2` keeps the terminal from answering, as its answers would arrive as key presses.
pub fn transmit(out: &mut impl Write, image: &Image) -> io::Result<()> {
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
        command(out, &controls, chunk)?;
        first = false;
    }
    Ok(())
}

/// Send an image for the program to place itself with [`place_at`], rather than with placeholders.
pub fn store(out: &mut impl Write, id: u32, png: &[u8]) -> io::Result<()> {
    let encoded = STANDARD.encode(png);
    let mut chunks = encoded.as_bytes().chunks(CHUNK).peekable();
    let mut first = true;
    while let Some(chunk) = chunks.next() {
        let more = u8::from(chunks.peek().is_some());
        let controls = if first {
            format!("a=t,f=100,i={id},q=2,m={more}")
        } else {
            format!("q=2,m={more}")
        };
        command(out, &controls, chunk)?;
        first = false;
    }
    Ok(())
}

/// Where an image is placed directly: at a character, this many pixels into it across, with this
/// many of its rows of pixels shown from the top.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    pub column: u16,
    pub row: u16,
    pub offset: usize,
    pub height: usize,
}

/// Place a stored image, or move it, as placing it again under the same placement id moves it.
/// An image can be placed in many places at once, each under a placement id of its own. It's
/// drawn above the text, and the terminal's cursor is put back where it was.
pub fn place_at(out: &mut impl Write, id: u32, placement: u32, at: Placement) -> io::Result<()> {
    let Placement {
        column,
        row,
        offset,
        height,
    } = at;
    write!(out, "\x1b7\x1b[{};{}H", row + 1, column + 1)?;
    command(
        out,
        &format!("a=p,i={id},p={placement},X={offset},h={height},C=1,z=1,q=2"),
        &[],
    )?;
    out.write_all(b"\x1b8")
}

/// Take one placement of an image off the screen, keeping the image to place again.
pub fn unplace(out: &mut impl Write, id: u32, placement: u32) -> io::Result<()> {
    command(out, &format!("a=d,d=i,i={id},p={placement},q=2"), &[])
}

/// Take every placement of an image off the screen, keeping the image to place again.
pub fn unplace_all(out: &mut impl Write, id: u32) -> io::Result<()> {
    command(out, &format!("a=d,d=i,i={id},q=2"), &[])
}

/// A mark in the sample page's ruler as a picture, for characters of this size: a triangle
/// pointing down at the bottom of a character, its tip as wide as its line in the waveform, so
/// placed over the line it sits on it. It's [`mark_reach`] pixels either side of the line across,
/// and a pixel more than that down, so its sides slope at 45 degrees, as `▾`'s do.
pub fn mark_png(kind: MarkKind, cell: (usize, usize)) -> Vec<u8> {
    mark_picture(kind, cell).png()
}

/// A mark as [`mark_png`] draws it, before it's a PNG.
fn mark_picture(kind: MarkKind, cell: (usize, usize)) -> Picture {
    let reach = mark_reach(cell.0);
    let width = kind.line_pixels() + 2 * reach;
    let height = cell.1.max(1);
    let mut picture = Picture::new(width, height, vec![rgb(kind.color())]);
    let rows = (reach + 1).min(height);
    for (row, y) in (height - rows..height).enumerate() {
        let inset = row + reach + 1 - rows;
        picture.pixels[y * width + inset..(y + 1) * width - inset].fill(1);
    }
    picture
}

/// An entry's points as the waveform draws them, where they are and nothing else, so changing a
/// slice's own settings or a letter doesn't send the picture again.
fn drawn_points(points: &Points) -> Points {
    let mut drawn = points.clone();
    drawn.marks.clear();
    for slice in &mut drawn.slices {
        slice.over = Default::default();
    }
    drawn
}

/// What the sample page's waveform or overview was drawn from, which is all that's compared to
/// tell if it has to be drawn again.
#[derive(Debug, Clone, PartialEq, Eq)]
struct WaveKey {
    hash: ContentHash,
    /// Whether the file's frames were there to draw closer than the analysis goes.
    loaded: bool,
    points: Points,
    first: u64,
    zoom: u64,
    columns: usize,
    rows: usize,
    cell: (usize, usize),
}

/// How wide the sample page's cursor is as a line, in pixels.
const CURSOR_LINE_PIXELS: usize = 2;

/// How wide each of the sample page's playheads is as a line, in pixels, as wide as the cursor's.
const PLAYHEAD_LINE_PIXELS: usize = 2;

/// A character's size in pixels until the terminal says: the author's screen's, kitty full screen
/// on a MacBook.
pub const DEFAULT_CELL: (usize, usize) = (15, 39);

/// The id the question about images asks with, which no image of the program's has.
const QUERY_ID: u32 = 31;
/// How long the terminal has to answer it. Both answer at once, so this is only for one that never
/// does.
const QUERY_WAIT: Duration = Duration::from_secs(2);

/// What the terminal said to the question about images, once it has said all of it: whether it
/// takes a PNG. The question is followed by one for the terminal's attributes, which every terminal
/// answers, and in order, so an answer that ends without an OK first is a no.
fn answer(reply: &[u8]) -> Option<bool> {
    let text = String::from_utf8_lossy(reply);
    let attributes = text.find("\x1b[?")?;
    text[attributes..]
        .contains('c')
        .then(|| text[..attributes].contains(&format!("\x1b_Gi={QUERY_ID};OK\x1b\\")))
}

/// Whether the terminal draws images with the graphics protocol. It asks the terminal to take a PNG
/// of one dot and not show it, as the program's own images are, and reads the answer before
/// anything else reads the keyboard.
pub fn supports_graphics() -> io::Result<bool> {
    let mut tty = OpenOptions::new().read(true).write(true).open("/dev/tty")?;
    let mut reader: File = tty.try_clone()?;
    terminal::enable_raw_mode()?;
    let dot = STANDARD.encode(png(&[vec![true]], [255; 3]));
    let asked = command(&mut tty, &format!("a=q,i={QUERY_ID},f=100"), dot.as_bytes())
        .and_then(|()| tty.write_all(b"\x1b[c"))
        .and_then(|()| tty.flush());
    let (send, receive) = mpsc::channel();
    // On its own thread, so a terminal that never answers can't hold up the program. If it
    // doesn't, the program stops, and the thread with it.
    std::thread::spawn(move || {
        let mut reply = Vec::new();
        let mut buffer = [0; 256];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(read) => reply.extend(&buffer[..read]),
            }
            if let Some(answer) = answer(&reply) {
                let _ = send.send(answer);
                return;
            }
        }
    });
    let supported = receive.recv_timeout(QUERY_WAIT).unwrap_or(false);
    terminal::disable_raw_mode()?;
    asked.map(|()| supported)
}

/// The images of a picker's preview: its lines' waveforms, a line each, and its own.
type PreviewImages = (Vec<Option<u32>>, Option<u32>);

/// The settings a filter curve is drawn from.
type FilterSettings = (FilterType, FilterMode, u8, u8, u8, u8, u8);

/// What an FX slot's drawing shows, which is all that's compared to tell if it's changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SlotDrawing {
    /// A compressor's curve, from its threshold and ratio.
    Compressor { threshold: u8, ratio: u8 },
    /// An EQ's curve, from all of its settings.
    Eq(Equaliser),
}

impl SlotDrawing {
    /// The drawing for an effect playing with these values, if it has one.
    fn of(insert: Insert, live: Option<[u8; MAX_PARAMS]>) -> Option<Self> {
        let playing = live.map_or(insert, |values| {
            (0..insert.params().len()).fold(insert, |playing, param| {
                playing.with_value(param, values[param])
            })
        });
        match playing {
            Insert::Compressor(settings) => Some(Self::Compressor {
                threshold: settings.threshold,
                ratio: settings.ratio,
            }),
            Insert::Eq(settings) => Some(Self::Eq(settings)),
            _ => None,
        }
    }

    /// How many characters across it is.
    fn width(self) -> usize {
        match self {
            Self::Compressor { .. } => DYNAMICS_CURVE_WIDTH,
            Self::Eq(_) => EQ_CURVE_WIDTH,
        }
    }

    fn png(self, color: [u8; 3], rate: u32) -> Vec<u8> {
        match self {
            Self::Eq(settings) => eq_png(settings, rate, color, self.width(), SLOT_DRAWING_HEIGHT),
            Self::Compressor { threshold, ratio } => compressor_png(
                Compressor {
                    threshold,
                    ratio,
                    ..Compressor::DEFAULT
                },
                color,
                self.width(),
                SLOT_DRAWING_HEIGHT,
            ),
        }
    }
}

/// A theme colour as the bytes of a picture's palette.
pub(crate) fn rgb(color: Color) -> [u8; 3] {
    match color {
        Color::Rgb(red, green, blue) => [red, green, blue],
        _ => [255; 3],
    }
}

/// The colour of a drawing, with the cursor on it or not.
fn color(active: bool) -> [u8; 3] {
    let style = if active {
        theme::active()
    } else {
        theme::dim()
    };
    match style.fg {
        Some(Color::Rgb(red, green, blue)) => [red, green, blue],
        _ => [255; 3],
    }
}

/// The images the terminal has. Each envelope keeps one with the cursor on it and one without, so
/// moving the cursor sends nothing.
///
/// A changed image gets a new id, rather than replacing the one with the old id. Kitty only looks at
/// placeholders on lines that have changed, and replacing an image disconnects them, so it would stay
/// blank. A new id changes the placeholders' colour, which makes kitty look again.
pub struct Graphics {
    base: u32,
    next: u32,
    images: [[Option<(Envelope, u32)>; 2]; TRACK_ENVELOPES],
    filter: [Option<(FilterSettings, u32)>; 2],
    /// The waveform of the sample the cursor's track plays. It's drawn once for each sample, which
    /// is held so another loaded later can't be mistaken for it.
    waveform: [Option<(Arc<SampleData>, u32)>; 2],
    /// Each FX slot's drawing, with the cursor on its slot or not, and the settings it shows.
    slots: [[Option<(SlotDrawing, u32)>; 2]; INSERTS],
    /// The scope's picture, as the rows it covers in each column, and its image.
    scope: Option<(Vec<(usize, usize)>, u32)>,
    /// The picker's preview and its images.
    preview: Option<(Arc<Shown>, PreviewImages)>,
    /// The sample page's waveform and its overview, with what each was drawn from.
    sample_wave: Option<(WaveKey, u32)>,
    sample_overview: Option<(WaveKey, u32)>,
    /// The image of the sample page's cursor as a line, and how tall it is in pixels.
    cursor_line: Option<(usize, u32)>,
    /// Where the line is placed, if it's on screen.
    cursor_placed: Option<Placement>,
    /// The image of the sample page's playhead as a line, and how tall it is in pixels, placed once
    /// for each playhead.
    playhead_line: Option<(usize, u32)>,
    /// Where each playhead's line is placed, by its placement id less one.
    playheads_placed: [Option<Placement>; PLAYHEADS],
    /// The images of the ruler's marks, one for each kind of point in [`MarkKind::ALL`]'s order,
    /// and the character size they're drawn for.
    mark_images: Option<((usize, usize), [u32; MarkKind::ALL.len()])>,
    /// Where each of the ruler's marks is placed, by its kind and its placement id.
    marks_placed: BTreeMap<(MarkKind, u32), Placement>,
    /// A character's size in pixels, which the sample page's pictures are drawn for.
    cell: (usize, usize),
    /// The window's size in characters, as last told.
    window: Option<(u16, u16)>,
    /// How many times the screen has been brought up to date, for how often the scope is.
    frames: usize,
    /// Images to forget once the screen has moved on from them.
    retired: Vec<u32>,
}

impl Graphics {
    /// Ready to send images, with ids another running copy won't be using.
    pub fn new() -> Self {
        Self {
            base: (std::process::id() & 0xFFFF) << 8,
            next: 0,
            images: [[None; 2]; TRACK_ENVELOPES],
            filter: [None; 2],
            waveform: [None, None],
            slots: [[None; 2]; INSERTS],
            scope: None,
            preview: None,
            sample_wave: None,
            sample_overview: None,
            cursor_line: None,
            cursor_placed: None,
            playhead_line: None,
            playheads_placed: [None; PLAYHEADS],
            mark_images: None,
            marks_placed: BTreeMap::new(),
            cell: DEFAULT_CELL,
            window: None,
            frames: 0,
            retired: Vec::new(),
        }
    }

    /// Draw the sample page's pictures for characters of this many pixels, as the terminal says
    /// its window has.
    pub fn use_cell(&mut self, cell: (usize, usize)) {
        if cell.0 > 0 && cell.1 > 0 {
            self.cell = cell;
        }
    }

    /// Take in the window's size as the terminal says it, in characters and pixels. ratatui clears
    /// a window that's changed size, and in kitty and Ghostty a clear frees an image that isn't
    /// shown through placeholders, so the cursor's line and the ruler's marks are sent again.
    pub fn use_window(&mut self, size: terminal::WindowSize) {
        self.use_cell((
            usize::from(size.width) / usize::from(size.columns.max(1)),
            usize::from(size.height) / usize::from(size.rows.max(1)),
        ));
        let characters = (size.columns, size.rows);
        if self.window.is_some_and(|was| was != characters) {
            self.retired
                .extend(self.cursor_line.take().map(|(_, id)| id));
            self.cursor_placed = None;
            self.retired
                .extend(self.playhead_line.take().map(|(_, id)| id));
            self.playheads_placed = [None; PLAYHEADS];
            self.retired
                .extend(self.mark_images.take().into_iter().flat_map(|(_, ids)| ids));
            self.marks_placed.clear();
        }
        self.window = Some(characters);
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
                || self.filter.iter().flatten().any(|image| image.1 == id)
                || self.waveform.iter().flatten().any(|image| image.1 == id)
                || self
                    .slots
                    .iter()
                    .flatten()
                    .flatten()
                    .any(|image| image.1 == id)
                || self.scope.as_ref().is_some_and(|image| image.1 == id)
                || self.sample_wave.as_ref().is_some_and(|image| image.1 == id)
                || self
                    .sample_overview
                    .as_ref()
                    .is_some_and(|image| image.1 == id)
                || self.cursor_line.is_some_and(|image| image.1 == id)
                || self.playhead_line.is_some_and(|image| image.1 == id)
                || self.mark_images.is_some_and(|(_, ids)| ids.contains(&id))
                || self
                    .preview
                    .iter()
                    .flat_map(|(_, (ids, wave))| ids.iter().chain(std::iter::once(wave)).flatten())
                    .any(|image| *image == id)
                || self.retired.contains(&id);
            if !in_use {
                return id;
            }
        }
    }

    /// Send the images the screen is about to show, if the terminal lacks them, and say which to use.
    pub fn sync(&mut self, app: &mut App, out: &mut impl Write) -> io::Result<()> {
        for id in self.retired.drain(..) {
            command(out, &format!("a=d,d=I,i={id},q=2"), &[])?;
        }
        if app.view == View::Track
            && let Some(track) = app.song().tracks.get(app.cursor.track).cloned()
        {
            match app.track_page {
                Page::Sound if track.is_sound() => self.sync_sound(app, &track, out)?,
                Page::Sound => {}
                Page::Mod => self.sync_mod(app, &track, out)?,
                Page::Fx => self.sync_fx(app, &track, out)?,
            }
        }
        if app.view == View::Sample {
            // So `h` and `l` move the cursor's line by a pixel of these.
            app.sample_page.cell_width = Some(self.cell.0);
            self.sync_sample(app, out)?;
        } else {
            app.sample_images = None;
        }
        self.sync_preview(app, out)?;
        if app.scope_shown && self.frames.is_multiple_of(SCOPE_FRAMES) {
            self.sync_scope(app, out)?;
        }
        self.frames = self.frames.wrapping_add(1);
        out.flush()
    }

    /// Place what the terminal places itself rather than through placeholders, once the screen
    /// is drawn: the sample page's ruler marks, its playheads and its cursor as lines, the
    /// cursor's last so it's over a playhead it meets. After the drawing, so they go where the
    /// page is now, and so a clear as the window changes size has already happened.
    pub fn place(&mut self, app: &App, out: &mut impl Write) -> io::Result<()> {
        self.sync_marks(app, out)?;
        self.sync_playheads(app, out)?;
        self.sync_cursor_line(app, out)?;
        out.flush()
    }

    /// Where a line at a frame goes on the sample page's waveform: at its pixel's character, the
    /// pixel's place in it across, the waveform's height down. None if it's out of view.
    fn line_at(&self, page: &SamplePage, screen: &WaveOnScreen, frame: u64) -> Option<Placement> {
        let across = self.cell.0 as u128;
        let after = frame.checked_sub(page.first_frame())?;
        let pixels = u128::from(after) * across / u128::from(page.zoom.max(1));
        let column = i32::try_from(i128::from(screen.left) + (pixels / across) as i128).ok()?;
        screen.columns.contains(&column).then(|| Placement {
            column: column as u16,
            row: screen.top,
            offset: (pixels % across) as usize,
            height: usize::from(screen.rows) * self.cell.1,
        })
    }

    /// Place the sample page's playheads as thin green lines at their frames, as the cursor's line
    /// is placed, or take them away when nothing plays, they're out of view, or the cursor is shown
    /// as its column, when they're their columns too. One image is placed once for each, so a
    /// playhead moving is placing it again, about 60 bytes, and nothing is sent while nothing
    /// moves, so a page with nothing playing sends nothing.
    fn sync_playheads(&mut self, app: &App, out: &mut impl Write) -> io::Result<()> {
        let page = &app.sample_page;
        let mut wanted = [None; PLAYHEADS];
        if let Some(screen) = &page.on_screen
            && lines_placed(app, std::time::Instant::now())
        {
            let placed = screen
                .playheads
                .iter()
                .filter_map(|frame| self.line_at(page, screen, *frame));
            for (slot, at) in wanted.iter_mut().zip(placed) {
                *slot = Some(at);
            }
        }
        if wanted == self.playheads_placed {
            return Ok(());
        }
        let height = usize::from(WAVE_ROWS) * self.cell.1;
        let id = match self.playhead_line.take() {
            Some((drawn, id)) if drawn == height => id,
            old => {
                // Drawn for another size: the old one goes, with its placements.
                if let Some((_, id)) = old {
                    command(out, &format!("a=d,d=I,i={id},q=2"), &[])?;
                }
                self.playheads_placed = [None; PLAYHEADS];
                let id = self.new_id();
                let mut line = Picture::new(PLAYHEAD_LINE_PIXELS, height, vec![rgb(theme::GREEN)]);
                line.pixels.fill(1);
                store(out, id, &line.png())?;
                id
            }
        };
        self.playhead_line = Some((height, id));
        for (index, (at, was)) in wanted.iter().zip(&self.playheads_placed).enumerate() {
            let placement = index as u32 + 1;
            match at {
                Some(at) if was != &Some(*at) => place_at(out, id, placement, *at)?,
                None if was.is_some() => unplace(out, id, placement)?,
                _ => {}
            }
        }
        self.playheads_placed = wanted;
        Ok(())
    }

    /// Send the waveforms in the picker's preview, if the terminal lacks them, and forget those of a
    /// preview it has moved on from. Only as many as the preview has room for are sent.
    fn sync_preview(&mut self, app: &mut App, out: &mut impl Write) -> io::Result<()> {
        let shown = app
            .picker
            .as_ref()
            .and_then(|picker| picker.preview.clone());
        let same = match (&self.preview, &shown) {
            (Some((held, _)), Some(shown)) => Arc::ptr_eq(held, shown),
            (None, None) => true,
            _ => false,
        };
        if !same {
            if let Some((_, (ids, wave))) = self.preview.take() {
                self.retired.extend(ids.into_iter().flatten());
                self.retired.extend(wave);
            }
            if let Some(shown) = shown {
                let mut ids = Vec::new();
                for line in shown.lines.iter().take(PREVIEW_ROWS) {
                    let Some(spans) = &line.wave else {
                        ids.push(None);
                        continue;
                    };
                    let id = self.new_id();
                    let png = spans_png(spans, PREVIEW_WAVE_HEIGHT, color(false));
                    self.send(out, id, &png, PREVIEW_WAVE_WIDTH, PREVIEW_WAVE_HEIGHT)?;
                    ids.push(Some(id));
                }
                let wave = match &shown.wave {
                    Some(spans) => {
                        let id = self.new_id();
                        let png = spans_png(spans, PREVIEW_SAMPLE_HEIGHT, color(false));
                        self.send(out, id, &png, PREVIEW_SAMPLE_WIDTH, PREVIEW_SAMPLE_HEIGHT)?;
                        Some(id)
                    }
                    None => None,
                };
                self.preview = Some((shown, (ids, wave)));
            }
        }
        if let (Some(picker), Some((_, (ids, wave)))) = (app.picker.as_mut(), &self.preview) {
            picker.preview_images.clone_from(ids);
            picker.preview_wave = *wave;
        }
        Ok(())
    }

    /// Send the Sound page's filter curve, if the terminal lacks it.
    fn sync_sound(&mut self, app: &mut App, track: &Track, out: &mut impl Write) -> io::Result<()> {
        // It shows what's playing, as a value's bar does.
        let (cutoff, resonance) = app.live.map_or((track.cutoff, track.resonance), |live| {
            (live.cutoff, live.resonance)
        });
        let settings = (
            track.filter,
            track.filter.plays(track.filter_mode),
            cutoff,
            resonance,
            track.drive,
            track.low_cut,
            track.high_cut,
        );
        let active = filter_being_edited(app);
        let held = self.filter[usize::from(active)];
        let id = match held {
            Some((drawn, id)) if drawn == settings => id,
            _ => {
                let id = self.new_id();
                let png = filter_png(
                    &Track {
                        cutoff,
                        resonance,
                        ..track.clone()
                    },
                    app.rate,
                    color(active),
                    FILTER_CURVE_WIDTH,
                    FILTER_CURVE_HEIGHT,
                );
                self.send(out, id, &png, FILTER_CURVE_WIDTH, FILTER_CURVE_HEIGHT)?;
                self.retired.extend(held.map(|(_, old)| old));
                self.filter[usize::from(active)] = Some((settings, id));
                id
            }
        };
        app.filter_image = Some(id);
        app.waveform_image = None;
        if track.machine == Machine::Sampler {
            self.sync_waveform(app, track, out)?;
        }
        Ok(())
    }

    /// Send the waveform of the sample a track plays, if the terminal lacks it. A track with no
    /// sample, or one that didn't load, has none.
    fn sync_waveform(
        &mut self,
        app: &mut App,
        track: &Track,
        out: &mut impl Write,
    ) -> io::Result<()> {
        let samples = Arc::clone(&app.samples);
        let song = Arc::clone(app.song());
        let Some(sample) = song
            .bank
            .get(track.sample())
            .and_then(BankEntry::source)
            .and_then(|source| samples.shared(samples.id_of(&source.path)?))
        else {
            return Ok(());
        };
        let active = app.track_control() == Control::Sample;
        let id = match self.waveform[usize::from(active)].take() {
            Some((drawn, id)) if Arc::ptr_eq(&drawn, sample) => {
                self.waveform[usize::from(active)] = Some((drawn, id));
                id
            }
            held => {
                self.retired.extend(held.map(|(_, old)| old));
                let id = self.new_id();
                let png = waveform_png(
                    &sample.frames,
                    color(active),
                    WAVEFORM_WIDTH,
                    WAVEFORM_HEIGHT,
                );
                self.send(out, id, &png, WAVEFORM_WIDTH, WAVEFORM_HEIGHT)?;
                self.waveform[usize::from(active)] = Some((Arc::clone(sample), id));
                id
            }
        };
        app.waveform_image = Some(id);
        Ok(())
    }

    /// Send the drawings of the FX page's slots, if the terminal lacks them: a compressor's or an
    /// EQ's curve.
    /// Each is drawn from the settings it shows, so it's sent again only when they change.
    fn sync_fx(&mut self, app: &mut App, track: &Track, out: &mut impl Write) -> io::Result<()> {
        let edited = slot_being_edited(app);
        let mut ids = [None; INSERTS];
        for (slot, insert) in track.inserts.iter().enumerate() {
            // It shows what's playing, as a value's bar does.
            let live = app.live.map(|live| live.inserts[slot]);
            let Some(shown) = SlotDrawing::of(*insert, live) else {
                continue;
            };
            let active = edited == Some(slot);
            let held = self.slots[slot][usize::from(active)];
            ids[slot] = Some(match held {
                Some((drawn, id)) if drawn == shown => id,
                _ => {
                    self.retired.extend(held.map(|(_, old)| old));
                    let id = self.new_id();
                    let png = shown.png(color(active), app.rate);
                    self.send(out, id, &png, shown.width(), SLOT_DRAWING_HEIGHT)?;
                    self.slots[slot][usize::from(active)] = Some((shown, id));
                    id
                }
            });
        }
        app.slot_images = ids;
        Ok(())
    }

    /// Send the scope's picture, if it has changed since the last one sent. A steady note, or
    /// silence, sends nothing.
    fn sync_scope(&mut self, app: &mut App, out: &mut impl Write) -> io::Result<()> {
        let spans = app.scope.spans(
            SCOPE_WIDTH * CELL_DOTS_ACROSS,
            SCOPE_HEIGHT * CELL_DOTS_DOWN,
        );
        let id = match self.scope.take() {
            Some((drawn, id)) if drawn == spans => {
                self.scope = Some((drawn, id));
                id
            }
            held => {
                // Retired first, so the new one can't be given its id.
                self.retired.extend(held.map(|(_, old)| old));
                let id = self.new_id();
                let png = scope_png(&spans, color(false));
                self.send(out, id, &png, SCOPE_WIDTH, SCOPE_HEIGHT)?;
                self.scope = Some((spans, id));
                id
            }
        };
        app.scope_image = Some(id);
        Ok(())
    }

    /// Send the sample page's waveform and overview, if the terminal lacks them. Each is drawn
    /// again only when what it shows changes: the zoom, the view, the points or the window, never
    /// for the cursor alone.
    fn sync_sample(&mut self, app: &mut App, out: &mut impl Write) -> io::Result<()> {
        let Ok(shown) = app.sample_page_here() else {
            app.sample_images = None;
            return Ok(());
        };
        let page = &app.sample_page;
        let wave = WaveKey {
            hash: shown.analysis.hash,
            loaded: shown.frames.is_some(),
            points: drawn_points(&shown.entry.edit.points),
            first: page.first_frame(),
            zoom: page.zoom,
            columns: page.columns,
            rows: usize::from(WAVE_ROWS),
            cell: self.cell,
        };
        let overview = WaveKey {
            first: 0,
            zoom: page.fit(shown.length()),
            rows: 1,
            ..wave.clone()
        };
        let mut ids = [0; 2];
        for (index, key) in [wave, overview].into_iter().enumerate() {
            let held = if index == 0 {
                self.sample_wave.take()
            } else {
                self.sample_overview.take()
            };
            let (key, id) = match held {
                Some((drawn, id)) if drawn == key => (drawn, id),
                old => {
                    // Retired first, so the new one can't be given its id.
                    self.retired.extend(old.map(|(_, id)| id));
                    let id = self.new_id();
                    let drawing = Drawing {
                        analysis: &shown.analysis,
                        frames: shown
                            .frames
                            .as_ref()
                            .map(|(data, rates)| (data.as_ref(), *rates)),
                        points: &key.points,
                        first: key.first,
                        zoom: key.zoom,
                        columns: key.columns,
                        rows: key.rows,
                        cell: key.cell,
                        merged: index == 1,
                    };
                    let png = drawing.picture().png();
                    self.send(out, id, &png, key.columns, key.rows)?;
                    (key, id)
                }
            };
            ids[index] = id;
            if index == 0 {
                self.sample_wave = Some((key, id));
            } else {
                self.sample_overview = Some((key, id));
            }
        }
        app.sample_images = Some((ids[0], ids[1]));
        Ok(())
    }

    /// Place the sample page's cursor as a thin line at its frame, or take it away when it isn't
    /// on screen, is shown as its column, or something is drawn over the page. It's an image of its
    /// own, sent once, and moving it is placing it again, so the waveform isn't sent again.
    fn sync_cursor_line(&mut self, app: &App, out: &mut impl Write) -> io::Result<()> {
        let page = &app.sample_page;
        let height = usize::from(WAVE_ROWS) * self.cell.1;
        let wanted = match &page.on_screen {
            Some(screen)
                if app.view == View::Sample
                    && page.cursor_line
                    && app.sample_images.is_some()
                    && !overlaid(app, std::time::Instant::now()) =>
            {
                self.line_at(page, screen, page.cursor.max(page.first_frame()))
            }
            _ => None,
        };
        if wanted == self.cursor_placed {
            return Ok(());
        }
        let Some(at) = wanted else {
            if let Some((_, id)) = self.cursor_line {
                unplace(out, id, 1)?;
            }
            self.cursor_placed = None;
            return Ok(());
        };
        let id = match self.cursor_line.take() {
            Some((drawn, id)) if drawn == height => id,
            old => {
                self.retired.extend(old.map(|(_, id)| id));
                let id = self.new_id();
                let mut line =
                    Picture::new(CURSOR_LINE_PIXELS, height, vec![rgb(theme::ROSEWATER)]);
                line.pixels.fill(1);
                store(out, id, &line.png())?;
                id
            }
        };
        self.cursor_line = Some((height, id));
        place_at(out, id, 1, at)?;
        self.cursor_placed = Some(at);
        Ok(())
    }

    /// Place the sample page's ruler marks, each a small triangle at its line's pixels, or take
    /// them away when the pictures aren't drawn or something is drawn over the page. Each kind of
    /// point's mark is an image of its own, sent once, and placed once for each of its points in
    /// view, so a scroll, a zoom or a moved point only places the marks that moved, and moving the
    /// cursor sends nothing.
    fn sync_marks(&mut self, app: &App, out: &mut impl Write) -> io::Result<()> {
        let page = &app.sample_page;
        let (across, down) = self.cell;
        let mut wanted = BTreeMap::new();
        if let Some(screen) = &page.on_screen
            && screen.top > 0
            && marks_placed(app, std::time::Instant::now())
        {
            let left = i64::from(screen.left);
            let shown = i64::from(screen.columns.start)..i64::from(screen.columns.end);
            for mark in &screen.marks {
                // Only a mark whose line is in view, as the waveform has it.
                let line = mark.line_pixel(page, across);
                if !shown.contains(&(left + line.div_euclid(across as i64))) {
                    continue;
                }
                let (first, _) = mark.pixels(page, across);
                let Ok(pixel) = usize::try_from(left * across as i64 + first) else {
                    continue;
                };
                wanted.insert(
                    (mark.kind, mark.placement),
                    Placement {
                        column: (pixel / across) as u16,
                        row: screen.top - 1,
                        offset: pixel % across,
                        height: down,
                    },
                );
            }
        }
        if wanted == self.marks_placed {
            return Ok(());
        }
        if wanted.is_empty() {
            // Taken off all at once, a command for each kind rather than each mark.
            if let Some((_, ids)) = self.mark_images {
                for kind in MarkKind::ALL {
                    if self.marks_placed.keys().any(|(placed, _)| *placed == kind) {
                        unplace_all(out, ids[kind.index()])?;
                    }
                }
            }
            self.marks_placed.clear();
            return Ok(());
        }
        let ids = match self.mark_images {
            Some((drawn, ids)) if drawn == self.cell => ids,
            old => {
                // Drawn for another size of character: the old ones go, with their placements.
                for id in old.into_iter().flat_map(|(_, ids)| ids) {
                    command(out, &format!("a=d,d=I,i={id},q=2"), &[])?;
                }
                self.marks_placed.clear();
                let mut ids = [0; MarkKind::ALL.len()];
                for kind in MarkKind::ALL {
                    // Held as each is given its id, so the next kind can't be given the same.
                    self.mark_images = Some((self.cell, ids));
                    let id = self.new_id();
                    store(out, id, &mark_png(kind, self.cell))?;
                    ids[kind.index()] = id;
                }
                self.mark_images = Some((self.cell, ids));
                ids
            }
        };
        for key in self.marks_placed.keys() {
            if !wanted.contains_key(key) {
                unplace(out, ids[key.0.index()], key.1)?;
            }
        }
        for (key, at) in &wanted {
            if self.marks_placed.get(key) != Some(at) {
                place_at(out, ids[key.0.index()], key.1, *at)?;
            }
        }
        self.marks_placed = wanted;
        Ok(())
    }

    /// Send the Mod page's envelopes, if the terminal lacks them. The LFOs' shapes are glyphs.
    fn sync_mod(&mut self, app: &mut App, track: &Track, out: &mut impl Write) -> io::Result<()> {
        let envelopes = track.envelopes;
        let edited = envelope_being_edited(app);
        let mut envelope_ids = [0; TRACK_ENVELOPES];
        for (slot, envelope) in envelopes.iter().enumerate() {
            let active = edited == Some(slot);
            let held = self.images[slot][usize::from(active)];
            envelope_ids[slot] = match held {
                Some((drawn, id)) if drawn == *envelope => id,
                _ => {
                    let id = self.new_id();
                    let png = envelope_png(envelope, color(active), DIAGRAM_WIDTH, DIAGRAM_HEIGHT);
                    self.send(out, id, &png, DIAGRAM_WIDTH, DIAGRAM_HEIGHT)?;
                    self.retired.extend(held.map(|(_, old)| old));
                    self.images[slot][usize::from(active)] = Some((*envelope, id));
                    id
                }
            };
        }
        app.envelope_images = Some(envelope_ids);
        Ok(())
    }

    /// Send one image.
    fn send(
        &self,
        out: &mut impl Write,
        id: u32,
        png: &[u8],
        columns: usize,
        rows: usize,
    ) -> io::Result<()> {
        let image = Image {
            id,
            png,
            columns,
            rows,
        };
        transmit(out, &image)
    }

    /// Have the terminal forget the images.
    pub fn forget(&self, out: &mut impl Write) -> io::Result<()> {
        let held = self.images.iter().flatten().flatten().map(|image| &image.1);
        let filter = self.filter.iter().flatten().map(|image| &image.1);
        let waveform = self.waveform.iter().flatten().map(|image| &image.1);
        let slots = self.slots.iter().flatten().flatten().map(|image| &image.1);
        let scope = self.scope.iter().map(|image| &image.1);
        let sample = self
            .sample_wave
            .iter()
            .chain(&self.sample_overview)
            .map(|image| &image.1)
            .chain(self.cursor_line.iter().map(|image| &image.1))
            .chain(self.playhead_line.iter().map(|image| &image.1))
            .chain(self.mark_images.iter().flat_map(|(_, ids)| ids));
        for id in held
            .chain(filter)
            .chain(waveform)
            .chain(slots)
            .chain(scope)
            .chain(sample)
            .chain(&self.retired)
        {
            command(out, &format!("a=d,d=I,i={id},q=2"), &[])?;
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

    /// A PNG's chunks, having checked their CRCs, and its pixels, each as the number of its colour
    /// in the palette.
    fn decode_colors(png: &[u8]) -> (Chunks, Vec<Vec<u8>>) {
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
        let depth = usize::from(header[8]);
        let idat = &chunks.iter().find(|(name, _)| name == "IDAT").unwrap().1;
        // The decoder checks the zlib data's own checksum, and what's stored or compressed.
        let mut lines = Vec::new();
        flate2::read::ZlibDecoder::new(&idat[..])
            .read_to_end(&mut lines)
            .unwrap();
        let stride = (width * depth).div_ceil(8) + 1;
        assert_eq!(lines.len() % stride, 0);
        let pixels = lines
            .chunks(stride)
            .map(|line| {
                assert_eq!(line[0], 0);
                (0..width)
                    .map(|x| {
                        let bit = x * depth;
                        (line[1 + bit / 8] >> (8 - depth - bit % 8)) & ((1 << depth) - 1) as u8
                    })
                    .collect()
            })
            .collect();
        (chunks, pixels)
    }

    /// A PNG's chunks and which of its pixels are drawn.
    fn decode(png: &[u8]) -> (Chunks, Vec<Vec<bool>>) {
        let (chunks, colors) = decode_colors(png);
        let pixels = colors
            .iter()
            .map(|row| row.iter().map(|color| *color != 0).collect())
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

    fn sent(png: &[u8]) -> Vec<u8> {
        let image = Image {
            id: 7,
            png,
            columns: 28,
            rows: 4,
        };
        let mut out = Vec::new();
        transmit(&mut out, &image).unwrap();
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
    fn the_terminal_takes_images_if_it_says_ok_before_its_attributes() {
        let attributes = "\x1b[?62;22c";
        let ok = format!("\x1b_Gi={QUERY_ID};OK\x1b\\{attributes}");
        assert_eq!(answer(ok.as_bytes()), Some(true));
        let refused = format!("\x1b_Gi={QUERY_ID};ENOTSUPPORTED:no\x1b\\{attributes}");
        assert_eq!(answer(refused.as_bytes()), Some(false));
        assert_eq!(answer(attributes.as_bytes()), Some(false));
        // Until the attributes have all come, it isn't over.
        assert_eq!(answer(&ok.as_bytes()[..ok.len() - 1]), None);
        assert_eq!(answer(b"\x1b_Gi=31;OK\x1b\\"), None);
        assert_eq!(answer(b""), None);
    }

    /// A filter curve's height at points across it, from 0 at the bottom to 1.
    fn curve(track: &Track) -> Vec<f32> {
        let level = |x: f32| {
            let db = track_response_db(track, 48_000, 20.0 * 1000_f32.powf(x));
            (db - CURVE_BOTTOM_DB) / (CURVE_TOP_DB - CURVE_BOTTOM_DB)
        };
        (0..=10).map(|step| level(step as f32 / 10.0)).collect()
    }

    fn open_track() -> Track {
        Track {
            filter: FilterType::LowPass,
            cutoff: 0xFF,
            resonance: 0,
            drive: 0,
            low_cut: 0,
            high_cut: 0xFF,
            ..Track::default()
        }
    }

    #[test]
    fn a_filter_curve_is_small_and_flat_when_the_filter_is_open() {
        let track = open_track();
        let png = filter_png(&track, 48_000, [255; 3], 24, 3);
        assert!(png.len() < 7000, "{}", png.len());
        let (_, pixels) = decode(&png);
        let (across, down) = (24 * CELL_DOTS_ACROSS, 3 * CELL_DOTS_DOWN);
        assert_eq!(
            (pixels[0].len(), pixels.len()),
            (across * DOT_PIXELS, down * DOT_PIXELS)
        );
        let flat = (0.0 - CURVE_BOTTOM_DB) / (CURVE_TOP_DB - CURVE_BOTTOM_DB);
        for level in curve(&track) {
            assert!((level - flat).abs() < 0.001, "{level}");
        }
    }

    #[test]
    fn a_filter_curve_goes_where_the_filter_does() {
        let mut track = Track {
            cutoff: 0x80,
            ..open_track()
        };
        let low = curve(&track);
        assert!(low[0] > low[10]);
        track.filter = FilterType::HighPass;
        let high = curve(&track);
        assert!(high[0] < high[10]);
        // Resonance raises a peak above the plain low-pass.
        track.filter = FilterType::LowPass;
        track.resonance = 0xC0;
        let peak = curve(&track).into_iter().fold(0.0, f32::max);
        assert!(peak > low.into_iter().fold(0.0, f32::max) + 0.1);
    }

    #[test]
    fn a_compressors_curve_is_the_compressors_own_sum() {
        let settings = Compressor {
            threshold: 0x80,
            ratio: 0xC0,
            ..Compressor::DEFAULT
        };
        let (columns, rows) = (4, 2);
        let (across, down) = (columns * CELL_DOTS_ACROSS, rows * CELL_DOTS_DOWN);
        let (_, pixels) = decode(&compressor_png(settings, [255; 3], columns, rows));
        assert_eq!(
            (pixels[0].len(), pixels.len()),
            (across * DOT_PIXELS, down * DOT_PIXELS)
        );
        // At the middle of each column of dots, the line covers the row the compressor's curve
        // puts it on.
        for column in 0..across {
            let input = DYNAMICS_RANGE_DB * ((column as f32 + 0.5) / across as f32 - 1.0);
            let out = curve_db(input, settings) / DYNAMICS_RANGE_DB + 1.0;
            let row = ((1.0 - out) * (down - 1) as f32).round() as usize;
            let x = column * DOT_PIXELS;
            let lit: Vec<usize> = (0..down)
                .filter(|row| pixels[row * DOT_PIXELS][x])
                .collect();
            assert!(
                lit.iter().any(|lit| lit.abs_diff(row) <= 1),
                "column {column}: {lit:?} against {row}"
            );
        }
        // Below the threshold it's the diagonal, from the bottom left, and above it, flatter.
        assert!(pixels[(down - 1) * DOT_PIXELS][0]);
        let top: Vec<usize> = (0..across)
            .map(|column| {
                (0..down)
                    .find(|row| pixels[row * DOT_PIXELS][column * DOT_PIXELS])
                    .unwrap()
            })
            .collect();
        assert!(top[across - 1] > 0, "a ratio over 1 keeps it off the top");
    }

    #[test]
    fn an_eqs_curve_is_the_eqs_own_sum_and_flat_when_its_flat() {
        let (columns, rows) = (10, 2);
        let (across, down) = (columns * CELL_DOTS_ACROSS, rows * CELL_DOTS_DOWN);
        let rows_lit = |settings: Equaliser| {
            let (_, pixels) = decode(&eq_png(settings, 48_000, [255; 3], columns, rows));
            assert_eq!(
                (pixels[0].len(), pixels.len()),
                (across * DOT_PIXELS, down * DOT_PIXELS)
            );
            (0..across)
                .map(|column| {
                    (0..down)
                        .filter(|row| pixels[row * DOT_PIXELS][column * DOT_PIXELS])
                        .collect::<Vec<usize>>()
                })
                .collect::<Vec<_>>()
        };
        let middle = ((down - 1) as f32 * 0.5).round() as usize;
        assert!(
            rows_lit(Equaliser::DEFAULT)
                .iter()
                .all(|lit| *lit == [middle])
        );
        // A boosted low end and a cut middle: each column covers where the EQ's sum puts it.
        let settings = Equaliser {
            low: 0xD0,
            mid: 0x50,
            ..Equaliser::DEFAULT
        };
        for (column, lit) in rows_lit(settings).iter().enumerate() {
            let x = (column as f32 + 0.5) / across as f32;
            let db = eq_response_db(settings, 48_000, 20.0 * 1000_f32.powf(x));
            let row = ((0.5 - db / (2.0 * EQ_RANGE_DB)) * (down - 1) as f32).round() as usize;
            assert!(
                lit.iter().any(|lit| lit.abs_diff(row) <= 1),
                "column {column}: {lit:?} against {row}"
            );
        }
    }

    #[test]
    fn the_scope_is_a_small_png_of_its_line() {
        let (across, down) = (
            SCOPE_WIDTH * CELL_DOTS_ACROSS,
            SCOPE_HEIGHT * CELL_DOTS_DOWN,
        );
        let spans: Vec<(usize, usize)> = (0..across)
            .map(|column| (column % down, down - 1))
            .collect();
        let png = scope_png(&spans, [255; 3]);
        assert!(png.len() < 4000, "{}", png.len());
        let (_, pixels) = decode(&png);
        assert_eq!(
            (pixels[0].len(), pixels.len()),
            (across * DOT_PIXELS, down * DOT_PIXELS)
        );
        for (column, (high, low)) in spans.iter().enumerate() {
            for row in 0..down {
                let pixel = pixels[row * DOT_PIXELS][column * DOT_PIXELS];
                assert_eq!(pixel, (*high..=*low).contains(&row), "dot {column}, {row}");
            }
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
        let out = String::from_utf8(sent(&pixels)).unwrap();
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
    fn the_marks_are_all_of_kittys_and_ghosttys_in_order() {
        // From kitty's `rowcolumn-diacritics.txt`, whose list Ghostty's `graphics_unicode.zig` has
        // too: 297 marks in rising order.
        assert_eq!(POSITIONS.len(), 297);
        assert!(POSITIONS.windows(2).all(|pair| pair[0] < pair[1]));
        for (index, mark) in [
            (0, '\u{305}'),
            (27, '\u{36D}'),
            (28, '\u{36E}'),
            (30, '\u{483}'),
            (100, '\u{7EB}'),
            (159, '\u{1CD1}'),
            (200, '\u{20D4}'),
            (296, '\u{1D244}'),
        ] {
            assert_eq!(POSITIONS[index], mark, "mark {index}");
        }
    }

    #[test]
    fn placeholders_past_28_cells_say_where_they_are() {
        let (width, height) = (160, 40);
        let lines = placeholder_lines(width, height);
        assert_eq!(lines.len(), height);
        for (row, line) in lines.iter().enumerate() {
            let chars: Vec<char> = line.chars().collect();
            for (column, cell) in chars.chunks(3).enumerate() {
                assert_eq!(cell, [PLACEHOLDER, POSITIONS[row], POSITIONS[column]]);
            }
            assert_eq!(ratatui::text::Line::raw(line.as_str()).width(), width);
        }
        let corner: Vec<char> = lines[30].chars().skip(159 * 3).collect();
        assert_eq!(corner, [PLACEHOLDER, '\u{483}', '\u{1CD1}']);
        let last = placeholder_lines(MOST_CELLS, MOST_CELLS);
        let corner: Vec<char> = last[MOST_CELLS - 1]
            .chars()
            .skip((MOST_CELLS - 1) * 3)
            .collect();
        assert_eq!(corner, [PLACEHOLDER, '\u{1D244}', '\u{1D244}']);
    }

    /// A picture with a stripe of each of its colours, under a wave.
    fn striped(width: usize, height: usize, colors: usize) -> Picture {
        let palette = (0..colors).map(|color| [color as u8, 100, 200]).collect();
        let mut picture = Picture::new(width, height, palette);
        for x in 0..width {
            let level = (x as f32 * 0.02).sin() * 0.5 + 0.5;
            let top = (level * (height - 1) as f32) as usize;
            for y in top..height {
                picture.pixels[y * width + x] = (1 + x * colors / width) as u8;
            }
        }
        picture
    }

    #[test]
    fn a_picture_takes_as_few_bits_as_its_colours_need() {
        for (colors, depth) in [(1, 1), (2, 2), (3, 2), (4, 4), (15, 4)] {
            let picture = striped(90, 20, colors);
            let (chunks, pixels) = decode_colors(&picture.png());
            assert_eq!(chunks[0].1[8], depth, "{colors} colours");
            assert_eq!(chunks[1].1.len(), 3 * (colors + 1));
            assert_eq!(chunks[2], ("tRNS".to_string(), vec![0]));
            assert_eq!(pixels.concat(), picture.pixels, "{colors} colours");
        }
    }

    #[test]
    fn a_big_picture_is_compressed_and_has_the_same_pixels() {
        // The sample page's waveform: 160 by 14 characters of 15 by 39 pixels, in four colours.
        let picture = striped(2400, 546, 4);
        let png = picture.png();
        let stored = (2400 / 2 + 1) * 546;
        assert!(png.len() < stored / 10, "{} of {stored}", png.len());
        let (_, pixels) = decode_colors(&png);
        assert_eq!(pixels.concat(), picture.pixels);

        // Lines of 64 bytes, a filter byte and 63 of pixels: 1023 of them fit in one stored block,
        // which says it's the last and isn't compressed, and 1024 don't.
        for (height, is_stored) in [(1023, true), (1024, false)] {
            let picture = striped(504, height, 1);
            let (chunks, pixels) = decode_colors(&picture.png());
            let idat = &chunks.iter().find(|(name, _)| name == "IDAT").unwrap().1;
            assert_eq!(idat[..3] == [0x78, 0x01, 1], is_stored, "{height} lines");
            assert_eq!(idat.len() > 64 * height, is_stored, "{height} lines");
            assert_eq!(pixels.concat(), picture.pixels);
        }
    }

    #[test]
    fn the_existing_drawings_are_unchanged() {
        // Each PNG's size and CRC, as they were before pictures could have more colours or be
        // compressed: the Sound page's waveform, the explorer's preview, the kit picker's and an
        // envelope.
        let frames: Vec<[f32; 2]> = (0..3000)
            .map(|frame| {
                let level = (frame as f32 * 0.05).sin() * (-(frame as f32) / 900.0).exp();
                [level, -level * 0.5]
            })
            .collect();
        let color = [200, 120, 40];
        let drawings = [
            waveform_png(&frames, color, WAVEFORM_WIDTH, WAVEFORM_HEIGHT),
            spans_png(
                &waveform_spans(&frames, PREVIEW_SAMPLE_WIDTH, PREVIEW_SAMPLE_HEIGHT),
                PREVIEW_SAMPLE_HEIGHT,
                color,
            ),
            spans_png(
                &waveform_spans(&frames, PREVIEW_WAVE_WIDTH, PREVIEW_WAVE_HEIGHT),
                PREVIEW_WAVE_HEIGHT,
                color,
            ),
            envelope_png(&adsr(1, 200, 0, 50), color, 28, 4),
        ];
        let before = [
            (6039, 0x5bff_5631),
            (9315, 0xf14b_3760),
            (783, 0x143f_a841),
            (9315, 0x8dc2_50f9),
        ];
        for (png, (length, crc)) in drawings.iter().zip(before) {
            assert_eq!((png.len(), crc32(&[png])), (length, crc));
        }
    }

    #[test]
    fn a_mark_is_a_triangle_at_the_bottom_of_a_character_with_its_tip_on_its_line() {
        for (kind, line) in [(MarkKind::Start, 2), (MarkKind::Slice, 1)] {
            let (_, pixels) = decode(&mark_png(kind, DEFAULT_CELL));
            // A third of the author's 15-pixel character either side of the line, and a
            // character tall, so it's placed in the ruler's row as it is.
            assert_eq!((pixels[0].len(), pixels.len()), (line + 10, 39));
            let lit =
                |row: &Vec<bool>| -> Vec<usize> { (0..row.len()).filter(|x| row[*x]).collect() };
            // The tip, at the bottom, is the line's pixels, 5 in.
            assert_eq!(lit(&pixels[38]), (5..5 + line).collect::<Vec<_>>());
            // Each row up is a pixel wider either side, to the whole width 6 rows up.
            for up in 0..=5 {
                assert_eq!(
                    lit(&pixels[38 - up]),
                    (5 - up..5 + line + up).collect::<Vec<_>>()
                );
            }
            assert!(
                pixels[..33].iter().flatten().all(|lit| !lit),
                "nothing above it"
            );
        }
    }
}
