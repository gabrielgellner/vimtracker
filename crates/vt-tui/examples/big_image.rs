//! Draws a waveform as big as the sample page's, 160 characters across and 14 down at one pixel for
//! each of the screen's, and measures what sending it costs each way the graphics protocol offers:
//! a PNG stored as it is, that PNG compressed with zlib (`o=z`), a PNG compressing its own data, as
//! `Picture::png` does, raw pixels compressed, and a stored PNG and raw pixels through shared memory
//! (`t=s`). Each is sent `RUNS` times, and the time is from starting to write it to the terminal
//! saying it has it, so it counts the terminal's decoding as well as the sending. The results are
//! in decision 93.
//!
//! It then shows the image, drawn for the terminal's own cells, with cell backgrounds under it as
//! the sample page's will have: a selection, a cursor, and a playhead that moves every frame, which
//! should show through the image's see-through pixels. Any key ends it, or `--seconds N`, and the
//! table is printed again on the way out.
//!
//! Run it in kitty or Ghostty, at the size the program is used at:
//!
//! ```sh
//! cargo run --release -p vt-tui --example big_image
//! ```

use std::ffi::CString;
use std::fs::OpenOptions;
use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use flate2::write::ZlibEncoder;
use flate2::{Compression, Crc};
use ratatui::crossterm::terminal;
use vt_tui::kitty::{Picture, id_color, placeholder_lines};

/// The size of the sample page's waveform, in characters.
const COLUMNS: usize = 160;
const ROWS: usize = 14;
/// A character's size in pixels on the author's screen, kitty full screen on a MacBook, which the
/// image is drawn for, one pixel for each of the screen's.
const CELL_PIXELS: (usize, usize) = (15, 39);
/// How many times each way is tried.
const RUNS: usize = 10;
/// The most base64 the protocol takes in one command.
const CHUNK: usize = 4096;
/// How long the terminal has to say it has an image.
const WAIT: Duration = Duration::from_secs(10);

/// The colours of the drawing, each with a meaning, as the sample page's would have.
const WAVE: u8 = 1;
const LOOPED: u8 = 2;
const SLICE: u8 = 3;
const MARK: u8 = 4;
const COLORS: [[u8; 3]; 4] = [
    [150, 150, 150],
    [230, 230, 230],
    [255, 170, 0],
    [90, 160, 255],
];
/// The cell backgrounds under it.
const SELECTION: [u8; 3] = [40, 60, 90];
const CURSOR: [u8; 3] = [90, 90, 40];
const PLAYHEAD: [u8; 3] = [110, 30, 30];

/// A way of sending the image.
#[derive(Clone, Copy)]
enum Way {
    /// A PNG, its data stored rather than compressed, as the program's small images are.
    Png,
    /// The same PNG, compressed with zlib at this level.
    PngZlib(u32),
    /// A PNG whose own data is compressed at this level, as most PNGs are.
    PngDeflated(u32),
    /// The program's own PNG, from `Picture::png`, which compresses a picture this big.
    Program,
    /// The pixels as they are, four bytes each, compressed with zlib at this level.
    RgbaZlib(u32),
    /// The stored PNG, in shared memory.
    SharedPng,
    /// The pixels as they are, in shared memory.
    SharedRgba,
}

impl Way {
    fn name(self) -> String {
        match self {
            Self::Png => "PNG, stored".into(),
            Self::PngZlib(level) => format!("PNG, o=z level {level}"),
            Self::PngDeflated(level) => format!("PNG deflated, {level}"),
            Self::Program => "Picture::png".into(),
            Self::RgbaZlib(level) => format!("RGBA, o=z level {level}"),
            Self::SharedPng => "PNG, t=s".into(),
            Self::SharedRgba => "RGBA, t=s".into(),
        }
    }
}

/// What one way cost, from its runs.
struct Cost {
    way: Way,
    bytes: usize,
    encode: Duration,
    /// Or what the terminal said instead of OK.
    round_trip: Result<Duration, String>,
}

/// A drum loop, four bars long, to draw: a kick on each beat, a snare on two and four, and a hat
/// between, at 120 beats a minute.
fn drums() -> Vec<f32> {
    let rate = 48_000;
    let beat = rate / 2;
    let mut noise = 0x1234_5678_u32;
    let mut frames = vec![0.0_f32; beat * 16];
    for (index, frame) in frames.iter_mut().enumerate() {
        noise = noise.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let white = (noise >> 8) as f32 / (1 << 23) as f32 - 1.0;
        let in_beat = (index % beat) as f32 / rate as f32;
        let in_half = (index % (beat / 2)) as f32 / rate as f32;
        let kick = (std::f32::consts::TAU * 55.0 * in_beat).sin() * (-in_beat * 9.0).exp();
        let snare = if (index / beat) % 2 == 1 {
            white * 0.6 * (-in_beat * 14.0).exp()
        } else {
            0.0
        };
        let hat = white * 0.2 * (-in_half * 60.0).exp();
        *frame = (kick * 0.9 + snare + hat).clamp(-1.0, 1.0);
    }
    frames
}

/// The waveform as a picture this many pixels across and down, with slices, a loop and marks.
fn draw(width: usize, height: usize) -> Picture {
    let frames = drums();
    let mut picture = Picture::new(width, height, COLORS.to_vec());
    let middle = height as f32 / 2.0;
    let (loop_start, loop_end) = (width * 5 / 8, width * 7 / 8);
    for x in 0..width {
        let part = &frames[x * frames.len() / width..(x + 1) * frames.len() / width];
        let (low, high) = part.iter().fold((0.0_f32, 0.0_f32), |(low, high), frame| {
            (low.min(*frame), high.max(*frame))
        });
        let top = (middle - high * middle).floor().max(0.0) as usize;
        let bottom = ((middle - low * middle).ceil() as usize).min(height - 1);
        let color = if (loop_start..loop_end).contains(&x) {
            LOOPED
        } else {
            WAVE
        };
        for y in top..=bottom {
            picture.pixels[y * width + x] = color;
        }
    }
    // A slice at each beat, two pixels wide, and a mark at the top of every fourth.
    for slice in 0..16 {
        let x = slice * width / 16;
        for y in 0..height {
            picture.pixels[y * width + x] = SLICE;
            picture.pixels[y * width + x + 1] = SLICE;
        }
        if slice % 4 == 0 {
            for y in 0..height / 20 {
                for dx in 0..(height / 20 - y) {
                    picture.pixels[y * width + x + 2 + dx] = MARK;
                }
            }
        }
    }
    picture
}

/// The picture as four bytes for each pixel, with nothing see-through.
fn rgba(picture: &Picture) -> Vec<u8> {
    picture
        .pixels
        .iter()
        .flat_map(|pixel| match pixel {
            0 => [0; 4],
            color => {
                let [red, green, blue] = picture.colors[usize::from(*color) - 1];
                [red, green, blue, 255]
            }
        })
        .collect()
}

fn zlib(data: &[u8], level: u32) -> Vec<u8> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::new(level));
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap()
}

/// The picture as a PNG with a palette, four bits to a pixel, its data compressed at this level of
/// zlib's, where `0` stores it as it is, as the program's small drawings are.
fn png(picture: &Picture, level: u32) -> Vec<u8> {
    let mut lines = Vec::with_capacity((picture.width.div_ceil(2) + 1) * picture.height);
    for row in picture.pixels.chunks(picture.width) {
        lines.push(0);
        lines.extend(
            row.chunks(2)
                .map(|pair| pair[0] << 4 | pair.get(1).copied().unwrap_or(0)),
        );
    }
    let mut header = Vec::new();
    header.extend((picture.width as u32).to_be_bytes());
    header.extend((picture.height as u32).to_be_bytes());
    header.extend([4, 3, 0, 0, 0]);
    let mut palette = vec![0; 3];
    palette.extend(picture.colors.iter().flatten());
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let chunks: [(&[u8; 4], Vec<u8>); 5] = [
        (b"IHDR", header),
        (b"PLTE", palette),
        (b"tRNS", vec![0]),
        (b"IDAT", zlib(&lines, level)),
        (b"IEND", Vec::new()),
    ];
    for (name, data) in chunks {
        let mut crc = Crc::new();
        crc.update(name);
        crc.update(&data);
        out.extend((data.len() as u32).to_be_bytes());
        out.extend(name);
        out.extend(&data);
        out.extend(crc.sum().to_be_bytes());
    }
    out
}

/// Put data in a new shared memory object, for the terminal to read and remove.
fn share(name: &str, data: &[u8]) -> io::Result<()> {
    let name = CString::new(name)?;
    // SAFETY: the name is a C string, the mapping is the object's length and written within it, and
    // each handle is closed.
    unsafe {
        let fd = libc::shm_open(
            name.as_ptr(),
            libc::O_CREAT | libc::O_EXCL | libc::O_RDWR,
            0o600,
        );
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        if libc::ftruncate(fd, data.len() as libc::off_t) != 0 {
            libc::close(fd);
            return Err(io::Error::last_os_error());
        }
        let map = libc::mmap(
            std::ptr::null_mut(),
            data.len(),
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_SHARED,
            fd,
            0,
        );
        libc::close(fd);
        if map == libc::MAP_FAILED {
            return Err(io::Error::last_os_error());
        }
        std::ptr::copy_nonoverlapping(data.as_ptr(), map.cast::<u8>(), data.len());
        libc::munmap(map, data.len());
    }
    Ok(())
}

/// A number for a shared memory object's name that no other in this run has.
fn shared_count() -> usize {
    static COUNT: AtomicUsize = AtomicUsize::new(0);
    COUNT.fetch_add(1, Ordering::Relaxed)
}

/// Remove a shared memory object the terminal didn't.
fn unshare(name: &str) {
    if let Ok(name) = CString::new(name) {
        // SAFETY: the name is a C string.
        unsafe { libc::shm_unlink(name.as_ptr()) };
    }
}

/// The commands that send an image in pieces, the first with these controls.
fn commands(controls: &str, payload: &[u8]) -> Vec<u8> {
    let encoded = STANDARD.encode(payload);
    let mut out = Vec::with_capacity(encoded.len() * 11 / 10);
    let mut chunks = encoded.as_bytes().chunks(CHUNK).peekable();
    let mut first = true;
    while let Some(chunk) = chunks.next() {
        let more = u8::from(chunks.peek().is_some());
        out.extend(b"\x1b_G");
        if first {
            out.extend(format!("{controls},m={more}").as_bytes());
        } else {
            out.extend(format!("m={more}").as_bytes());
        }
        out.push(b';');
        out.extend(chunk);
        out.extend(b"\x1b\\");
        first = false;
    }
    out
}

/// What to write to send the picture one way, with the image id and how to place it, and the
/// shared memory object it uses, if any.
fn prepare(way: Way, picture: &Picture, controls: &str) -> (Vec<u8>, Option<String>) {
    let size = format!("s={},v={}", picture.width, picture.height);
    match way {
        Way::Png => (
            commands(&format!("{controls},f=100"), &png(picture, 0)),
            None,
        ),
        Way::PngZlib(level) => {
            let png = png(picture, 0);
            (
                commands(
                    &format!("{controls},f=100,o=z,S={}", png.len()),
                    &zlib(&png, level),
                ),
                None,
            )
        }
        Way::PngDeflated(level) => (
            commands(&format!("{controls},f=100"), &png(picture, level)),
            None,
        ),
        Way::Program => (commands(&format!("{controls},f=100"), &picture.png()), None),
        Way::RgbaZlib(level) => (
            commands(
                &format!("{controls},f=32,{size},o=z"),
                &zlib(&rgba(picture), level),
            ),
            None,
        ),
        Way::SharedPng | Way::SharedRgba => {
            let (data, format) = match way {
                Way::SharedPng => (png(picture, 0), "f=100".to_string()),
                _ => (rgba(picture), format!("f=32,{size}")),
            };
            let name = format!("/vt-big-{}-{}", std::process::id(), shared_count());
            match share(&name, &data) {
                Ok(()) => (
                    commands(
                        &format!("{controls},{format},t=s,S={}", data.len()),
                        name.as_bytes(),
                    ),
                    Some(name),
                ),
                Err(_) => (Vec::new(), None),
            }
        }
    }
}

/// Wait for the terminal's answer about this image, keeping what else it sent.
fn answer(input: &Receiver<Vec<u8>>, pending: &mut Vec<u8>, id: u32) -> Result<(), String> {
    let deadline = Instant::now() + WAIT;
    let start = format!("\x1b_Gi={id};");
    loop {
        let text = String::from_utf8_lossy(pending).into_owned();
        if let Some(at) = text.find(&start)
            && let Some(end) = text[at..].find("\x1b\\")
        {
            let reply = text[at + start.len()..at + end].to_string();
            pending.drain(..at + end + 2);
            return if reply == "OK" { Ok(()) } else { Err(reply) };
        }
        let left = deadline
            .checked_duration_since(Instant::now())
            .ok_or("no answer")?;
        pending.extend(input.recv_timeout(left).map_err(|_| "no answer")?);
    }
}

fn median(mut times: Vec<Duration>) -> Duration {
    times.sort();
    times[times.len() / 2]
}

/// Send the picture each way, `RUNS` times, and say what it cost.
fn measure(
    tty: &mut impl Write,
    input: &Receiver<Vec<u8>>,
    picture: &Picture,
    ways: &[Way],
) -> Vec<Cost> {
    let mut pending = Vec::new();
    let mut id = 0x00_7E_00;
    ways.iter()
        .map(|&way| {
            let (mut encodes, mut trips, mut bytes) = (Vec::new(), Vec::new(), 0);
            let mut failed = None;
            for _ in 0..RUNS {
                id += 1;
                let started = Instant::now();
                let (out, shared) = prepare(way, picture, &format!("a=t,i={id},q=0"));
                encodes.push(started.elapsed());
                bytes = out.len();
                if out.is_empty() {
                    failed = Some("no shared memory".to_string());
                    break;
                }
                let sent = Instant::now();
                tty.write_all(&out).unwrap();
                tty.flush().unwrap();
                let reply = answer(input, &mut pending, id);
                trips.push(sent.elapsed());
                write!(tty, "\x1b_Ga=d,d=I,i={id},q=2\x1b\\").unwrap();
                if let Err(reply) = reply {
                    if let Some(name) = shared {
                        unshare(&name);
                    }
                    failed = Some(reply);
                    break;
                }
            }
            Cost {
                way,
                bytes,
                encode: median(encodes),
                round_trip: match failed {
                    Some(reply) => Err(reply),
                    None => Ok(median(trips)),
                },
            }
        })
        .collect()
}

fn table(costs: &[Cost], picture: &Picture, cell: (usize, usize)) -> Vec<String> {
    let mut lines = vec![
        format!(
            "{COLUMNS}x{ROWS} cells of {}x{} pixels: {}x{} pixels, {} colours. Median of {RUNS}. \
             This terminal's cells are {}x{}.",
            CELL_PIXELS.0,
            CELL_PIXELS.1,
            picture.width,
            picture.height,
            picture.colors.len(),
            cell.0,
            cell.1,
        ),
        format!(
            "{:<20} {:>12} {:>12} {:>14}",
            "Way", "Bytes sent", "Encode ms", "Round trip ms"
        ),
    ];
    for cost in costs {
        let trip = match &cost.round_trip {
            Ok(trip) => format!("{:.2}", trip.as_secs_f64() * 1000.0),
            Err(reply) => reply.chars().take(40).collect(),
        };
        lines.push(format!(
            "{:<20} {:>12} {:>12.2} {:>14}",
            cost.way.name(),
            cost.bytes,
            cost.encode.as_secs_f64() * 1000.0,
            trip
        ));
    }
    lines
}

/// Draw the placeholders, with this frame's backgrounds behind them.
fn frame(tty: &mut impl Write, id: u32, frame: usize) -> io::Result<()> {
    let ratatui::style::Color::Rgb(red, green, blue) = id_color(id) else {
        unreachable!()
    };
    let playhead = frame % COLUMNS;
    let mut out = String::new();
    for (row, line) in placeholder_lines(COLUMNS, ROWS).iter().enumerate() {
        out.push_str(&format!(
            "\x1b[{};1H\x1b[38;2;{red};{green};{blue}m",
            row + 1
        ));
        for (column, cell) in line.chars().collect::<Vec<_>>().chunks(3).enumerate() {
            let background = if column == playhead {
                Some(PLAYHEAD)
            } else if column == 100 {
                Some(CURSOR)
            } else if (40..64).contains(&column) {
                Some(SELECTION)
            } else {
                None
            };
            match background {
                Some([r, g, b]) => out.push_str(&format!("\x1b[48;2;{r};{g};{b}m")),
                None => out.push_str("\x1b[49m"),
            }
            out.extend(cell);
        }
        out.push_str("\x1b[0m");
    }
    tty.write_all(out.as_bytes())?;
    tty.flush()
}

fn main() -> io::Result<()> {
    let seconds = std::env::args()
        .skip_while(|arg| arg != "--seconds")
        .nth(1)
        .and_then(|seconds| seconds.parse::<u64>().ok());
    let size = terminal::window_size()?;
    let cell = (
        usize::from(size.width) / usize::from(size.columns).max(1),
        usize::from(size.height) / usize::from(size.rows).max(1),
    );
    let picture = draw(COLUMNS * CELL_PIXELS.0, ROWS * CELL_PIXELS.1);

    let mut tty = OpenOptions::new().read(true).write(true).open("/dev/tty")?;
    let mut reader = tty.try_clone()?;
    terminal::enable_raw_mode()?;
    let (send, input) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buffer = [0; 4096];
        while let Ok(read) = reader.read(&mut buffer) {
            if read == 0 || send.send(buffer[..read].to_vec()).is_err() {
                break;
            }
        }
    });
    write!(tty, "\x1b[2J\x1b[H\x1b[?25lMeasuring...")?;
    tty.flush()?;
    let ways = [
        Way::Png,
        Way::PngZlib(1),
        Way::PngZlib(6),
        Way::PngDeflated(1),
        Way::PngDeflated(6),
        Way::Program,
        Way::RgbaZlib(1),
        Way::SharedPng,
        Way::SharedRgba,
    ];
    let costs = measure(&mut tty, &input, &picture, &ways);
    let lines = table(&costs, &picture, cell);

    // Show it, drawn for this terminal's cells so it fills them, until a key or the time is up.
    let id = 0x00_7D_01;
    let shown = if cell.0 > 0 && cell.1 > 0 {
        draw(COLUMNS * cell.0, ROWS * cell.1)
    } else {
        picture
    };
    let (out, _) = prepare(
        Way::Program,
        &shown,
        &format!("a=T,U=1,i={id},c={COLUMNS},r={ROWS},q=2"),
    );
    write!(tty, "\x1b[2J")?;
    tty.write_all(&out)?;
    for (row, line) in lines.iter().enumerate() {
        write!(tty, "\x1b[{};1H{line}", ROWS + 2 + row)?;
    }
    write!(
        tty,
        "\x1b[{};1HUnder the image: a selection (blue), a cursor (yellow) and a playhead (red) \
         moving every frame. Any key ends it.",
        ROWS + 3 + lines.len()
    )?;
    let started = Instant::now();
    let mut count = 0;
    while input.try_recv().is_err()
        && seconds.is_none_or(|seconds| started.elapsed() < Duration::from_secs(seconds))
    {
        frame(&mut tty, id, count)?;
        count += 1;
        std::thread::sleep(Duration::from_millis(33));
    }
    write!(tty, "\x1b_Ga=d,d=I,i={id},q=2\x1b\\\x1b[2J\x1b[H\x1b[?25h")?;
    tty.flush()?;
    terminal::disable_raw_mode()?;
    for line in lines {
        println!("{line}");
    }
    Ok(())
}
