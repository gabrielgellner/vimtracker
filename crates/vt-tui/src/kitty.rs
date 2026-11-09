//! Small drawings in pixels, using kitty's graphics protocol, which kitty and Ghostty both have. An
//! image is sent ahead of time, and placeholder characters say where it goes. As they're text, a
//! window can be drawn over them.

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use ratatui::crossterm::terminal;
use ratatui::style::Color;
use vt_engine::dsp::compressor::curve_db;
use vt_engine::dsp::eq::response_db as eq_response_db;
use vt_engine::dsp::filter::track_response_db;
use vt_model::constants::TRACK_ENVELOPES;
use vt_model::envelope::Envelope;
use vt_model::insert::{Compressor, Equaliser, INSERTS, Insert, MAX_PARAMS};
use vt_model::lfo::{LFOS_PER_TRACK, Shape, wave};
use vt_model::sample::SampleData;
use vt_model::song::{FilterMode, FilterType, Machine, Track};

use crate::app::App;
use crate::input::mode::View;
use crate::input::picker::Shown;
use crate::theme;
use crate::track_page::{Control, Page};
use crate::views::curve::{envelope_columns, line_columns};
use crate::views::meter::{SCOPE_HEIGHT, SCOPE_WIDTH};
use crate::views::picker::{PREVIEW_ROWS, PREVIEW_WAVE_HEIGHT, PREVIEW_WAVE_WIDTH};
use crate::views::track::{
    DIAGRAM_HEIGHT, DIAGRAM_WIDTH, DYNAMICS_CURVE_WIDTH, EQ_CURVE_WIDTH, FILTER_CURVE_HEIGHT,
    FILTER_CURVE_WIDTH, LFO_ICON_WIDTH, SLOT_DRAWING_HEIGHT, WAVEFORM_HEIGHT, WAVEFORM_WIDTH,
    envelope_being_edited, filter_being_edited, lfo_being_edited, slot_being_edited,
};
use crate::waveform;

/// The character that stands for a piece of an image.
pub(crate) const PLACEHOLDER: char = '\u{10EEEE}';
/// The marks that give a placeholder's row and column, from kitty's `rowcolumn-diacritics.txt`.
pub(crate) const POSITIONS: [char; 28] = [
    '\u{305}', '\u{30D}', '\u{30E}', '\u{310}', '\u{312}', '\u{33D}', '\u{33E}', '\u{33F}',
    '\u{346}', '\u{34A}', '\u{34B}', '\u{34C}', '\u{350}', '\u{351}', '\u{352}', '\u{357}',
    '\u{35B}', '\u{363}', '\u{364}', '\u{365}', '\u{366}', '\u{367}', '\u{368}', '\u{369}',
    '\u{36A}', '\u{36B}', '\u{36C}', '\u{36D}',
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

/// How much of a row an LFO's icon stays inside, so the icons of two rows don't touch.
const ICON_HEIGHT: f32 = 0.8;
/// The random values an LFO's icon shows, and how many a random shape's icon shows.
const ICON_KEY: u64 = 148;
const RANDOM_ICON_CYCLES: f64 = 4.0;

/// What a filter curve's height covers, in dB, with the frequency across it from 20 Hz to 20 kHz. It
/// has more room below than above, as a filter takes away more than it adds.
const CURVE_BOTTOM_DB: f32 = -36.0;
const CURVE_TOP_DB: f32 = 24.0;

/// How far down a compressor's curve goes, in dB, both in and out, from `0` at the top and right.
/// It's as far down as the threshold goes.
const DYNAMICS_RANGE_DB: f32 = 60.0;

/// How far up and down an EQ's curve goes, in dB.
const EQ_RANGE_DB: f32 = 18.0;

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

/// An LFO's wave as a PNG, to show which shape it is. It uses the real wave, with one cycle or, for
/// a random one, a few, since one cycle of `Random` is only flat.
pub fn shape_png(shape: Shape, color: [u8; 3], columns: usize, rows: usize) -> Vec<u8> {
    let cycles = match shape {
        Shape::Random | Shape::Smooth => RANDOM_ICON_CYCLES,
        _ => 1.0,
    };
    let level = |x: f32| {
        // Stopping just short of the end, or a saw would jump back up there.
        let value = wave(shape, f64::from(x.min(0.999)) * cycles, ICON_KEY);
        0.5 + value * 0.5 * ICON_HEIGHT
    };
    let (across, down) = (columns * CELL_DOTS_ACROSS, rows * CELL_DOTS_DOWN);
    line_png(&line_columns(level, level(0.0), across, down), down, color)
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
/// moving the cursor sends nothing, and each LFO shape has the same two, sent once when first shown.
///
/// A changed image gets a new id, rather than replacing the one with the old id. Kitty only looks at
/// placeholders on lines that have changed, and replacing an image disconnects them, so it would stay
/// blank. A new id changes the placeholders' colour, which makes kitty look again.
pub struct Graphics {
    base: u32,
    next: u32,
    images: [[Option<(Envelope, u32)>; 2]; TRACK_ENVELOPES],
    shapes: [[Option<u32>; 2]; Shape::ALL.len()],
    filter: [Option<(FilterSettings, u32)>; 2],
    /// The waveform of the sample the cursor's track plays. It's drawn once for each sample, which
    /// is held so another loaded later can't be mistaken for it.
    waveform: [Option<(Arc<SampleData>, u32)>; 2],
    /// Each FX slot's drawing, with the cursor on its slot or not, and the settings it shows.
    slots: [[Option<(SlotDrawing, u32)>; 2]; INSERTS],
    /// The scope's picture, as the rows it covers in each column, and its image.
    scope: Option<(Vec<(usize, usize)>, u32)>,
    /// The picker's preview, and the images of its waveforms, a line each.
    preview: Option<(Arc<Shown>, Vec<Option<u32>>)>,
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
            shapes: [[None; 2]; Shape::ALL.len()],
            filter: [None; 2],
            waveform: [None, None],
            slots: [[None; 2]; INSERTS],
            scope: None,
            preview: None,
            frames: 0,
            retired: Vec::new(),
        }
    }

    /// An id no image has at the moment.
    fn new_id(&mut self) -> u32 {
        loop {
            self.next = self.next % 0xFF + 1;
            let id = self.base | self.next;
            let in_use = self
                .shapes
                .iter()
                .flatten()
                .flatten()
                .any(|shape| *shape == id)
                || self
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
                || self
                    .preview
                    .iter()
                    .flat_map(|(_, ids)| ids.iter().flatten())
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
        self.sync_preview(app, out)?;
        if app.scope_shown && self.frames.is_multiple_of(SCOPE_FRAMES) {
            self.sync_scope(app, out)?;
        }
        self.frames = self.frames.wrapping_add(1);
        out.flush()
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
            if let Some((_, ids)) = self.preview.take() {
                self.retired.extend(ids.into_iter().flatten());
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
                self.preview = Some((shown, ids));
            }
        }
        if let (Some(picker), Some((_, ids))) = (app.picker.as_mut(), &self.preview) {
            picker.preview_images.clone_from(ids);
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
        if track.machine == Machine::Sample {
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
        let Some(sample) = track
            .sample
            .as_deref()
            .and_then(|path| samples.shared(samples.id_of(path)?))
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

    /// Send the Mod page's envelopes and LFO shapes, if the terminal lacks them.
    fn sync_mod(&mut self, app: &mut App, track: &Track, out: &mut impl Write) -> io::Result<()> {
        let (envelopes, lfos) = (track.envelopes, track.lfos);
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
        let edited = lfo_being_edited(app);
        let mut lfo_ids = [0; LFOS_PER_TRACK];
        for (slot, lfo) in lfos.iter().enumerate() {
            let active = edited == Some(slot);
            let index = Shape::ALL
                .iter()
                .position(|shape| *shape == lfo.shape)
                .unwrap_or(0);
            lfo_ids[slot] = match self.shapes[index][usize::from(active)] {
                Some(id) => id,
                None => {
                    let id = self.new_id();
                    let png = shape_png(lfo.shape, color(active), LFO_ICON_WIDTH, 1);
                    self.send(out, id, &png, LFO_ICON_WIDTH, 1)?;
                    self.shapes[index][usize::from(active)] = Some(id);
                    id
                }
            };
        }
        app.envelope_images = Some(envelope_ids);
        app.lfo_images = Some(lfo_ids);
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
        let shapes = self.shapes.iter().flatten().flatten();
        let filter = self.filter.iter().flatten().map(|image| &image.1);
        let waveform = self.waveform.iter().flatten().map(|image| &image.1);
        let slots = self.slots.iter().flatten().flatten().map(|image| &image.1);
        let scope = self.scope.iter().map(|image| &image.1);
        for id in held
            .chain(shapes)
            .chain(filter)
            .chain(waveform)
            .chain(slots)
            .chain(scope)
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

    #[test]
    fn each_lfo_shape_has_its_own_small_icon() {
        let icons = Shape::ALL.map(|shape| shape_png(shape, [255; 3], 4, 1));
        for (index, icon) in icons.iter().enumerate() {
            assert!(icon.len() < 1000, "{}", icon.len());
            let (_, pixels) = decode(icon);
            let (across, down) = (4 * CELL_DOTS_ACROSS, CELL_DOTS_DOWN);
            assert_eq!(
                (pixels[0].len(), pixels.len()),
                (across * DOT_PIXELS, down * DOT_PIXELS)
            );
            assert!(
                !icons[index + 1..].contains(icon),
                "{:?}",
                Shape::ALL[index]
            );
            // It stays off the top and bottom of its row, so two rows' icons don't touch.
            assert!(pixels[0].iter().all(|pixel| !pixel));
            assert!(pixels[down * DOT_PIXELS - 1].iter().all(|pixel| !pixel));
        }
    }

    #[test]
    fn a_saw_falls_and_a_ramp_rises_with_no_jump_at_the_end() {
        let spans = |shape| {
            let level = |x: f32| 0.5 + wave(shape, f64::from(x.min(0.999)), 0) * 0.4;
            line_columns(level, level(0.0), 24, 12)
        };
        let (saw, ramp) = (spans(Shape::Saw), spans(Shape::Ramp));
        assert!(saw[0].0 < saw[23].0);
        assert!(ramp[0].0 > ramp[23].0);
        assert_eq!(saw[23].0, saw[23].1, "a single dot, with no line back up");
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
}
