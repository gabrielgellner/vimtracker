//! What the sample page costs on a five-minute file, at every zoom: the time from a key to the
//! screen written, and the bytes written, for zooming in from the whole sample a step at a time,
//! scrolling a column, and moving the cursor, drawn as a line or as its column. The results are in
//! decision 102.
//!
//! The program's own path is timed: the key, `Graphics::sync` drawing and sending the pictures,
//! and ratatui drawing the screen, written to memory. In kitty or Ghostty it then sends each
//! zoom's waveform to the terminal and times until the terminal says it has decoded it, which is
//! the rest of the way to the image on screen. Run it at the size the program is used at:
//!
//! ```sh
//! cargo run --release -p vt-tui --example sample_page_cost
//! cargo run --release -p vt-tui --example sample_page_cost -- --file five-minutes.wav --out table.txt
//! ```

use std::fs::OpenOptions;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::terminal;
use ratatui::layout::Rect;
use ratatui::{Terminal, TerminalOptions, Viewport};
use vt_engine::engine::Engine;
use vt_engine::link::connect;
use vt_engine::published::Published;
use vt_model::analysis::Analysis;
use vt_model::bank::{BankEntry, ContentHash};
use vt_model::sample::{Decoded, SampleData, decode_sample};
use vt_model::samples::SamplePool;
use vt_model::settings::SettingId;
use vt_model::song::{Machine, Song};
use vt_tui::app::App;
use vt_tui::input::keys::parse_sequence;
use vt_tui::kitty::{DEFAULT_CELL, Graphics};
use vt_tui::run::draw_screen;
use vt_tui::sample_wave::Drawing;

/// The author's screen, in characters.
const SCREEN: (u16, u16) = (163, 42);
const RATE: u32 = 44_100;
/// How many times each is done, for the median.
const RUNS: usize = 9;
/// The most base64 the protocol takes in one command.
const CHUNK: usize = 4096;
/// How long the terminal has to say it has an image.
const WAIT: Duration = Duration::from_secs(10);

/// Five minutes of drums, a bar at a time getting quieter and louder in turns, so every zoom has
/// something to show.
fn five_minutes() -> Decoded {
    let frames = RATE as usize * 300;
    let beat = RATE as usize / 2;
    let mut noise = 0x1234_5678_u32;
    let mut samples = Vec::with_capacity(frames * 2);
    for index in 0..frames {
        noise = noise.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let white = (noise >> 8) as f32 / (1 << 23) as f32 - 1.0;
        let in_beat = (index % beat) as f32 / RATE as f32;
        let in_half = (index % (beat / 2)) as f32 / RATE as f32;
        let kick = (std::f32::consts::TAU * 55.0 * in_beat).sin() * (-in_beat * 9.0).exp();
        let snare = if (index / beat) % 2 == 1 {
            white * 0.5 * (-in_beat * 14.0).exp()
        } else {
            0.0
        };
        let hat = white * 0.25 * (-in_half * 60.0).exp();
        let level = [1.0, 0.8, 0.5, 0.15][(index / (beat * 4)) % 4];
        let left = (kick * 0.8 + snare + hat * 0.4) * level;
        let right = (kick * 0.8 + snare + hat) * level;
        samples.extend([left, right]);
    }
    Decoded {
        samples,
        channels: 2,
        rate: RATE,
    }
}

fn median(mut times: Vec<Duration>) -> Duration {
    times.sort();
    times[times.len() / 2]
}

fn ms(time: Duration) -> f64 {
    time.as_secs_f64() * 1000.0
}

/// What's written, counted, and thrown away.
struct Counter(usize);

impl Write for Counter {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.0 += data.len();
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// How many bytes ratatui has written for the screen, as its backend keeps its writer to itself.
static SCREEN_WRITTEN: AtomicUsize = AtomicUsize::new(0);

/// What ratatui writes, counted in [`SCREEN_WRITTEN`] and thrown away.
struct Screen;

impl Write for Screen {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        SCREEN_WRITTEN.fetch_add(data.len(), Ordering::Relaxed);
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// The program on the sample page, with the file as its first track's sample.
fn app(decoded: &Decoded) -> App {
    let hash = ContentHash(0x5A_3B1E);
    let mut pool = SamplePool::default();
    let stereo: Vec<[f32; 2]> = if decoded.channels == 1 {
        decoded.samples.iter().map(|sample| [*sample; 2]).collect()
    } else {
        decoded.samples.as_chunks::<2>().0.to_vec()
    };
    pool.insert("five-minutes.wav", SampleData { frames: stereo });
    let started = Instant::now();
    pool.insert_analysis(Arc::new(Analysis::of(hash, decoded)));
    println!(
        "Analysing {:.1} s of audio took {:.0} ms, once, kept in the cache after.",
        decoded.info().seconds(),
        ms(started.elapsed())
    );
    let mut song = Song::new(2);
    let number = song
        .bank
        .add(BankEntry::of_file("five-minutes.wav", hash))
        .expect("room");
    song.tracks[0].machine = Machine::Sampler;
    song.tracks[0].set_setting(SettingId::SAMPLE, number);
    let samples = Arc::new(pool);
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, realtime) = connect(Engine::new(decoded.rate, version));
    // The engine's side isn't run, so it's let go of rather than dropped while it's still wanted.
    std::mem::forget(realtime);
    App::new(song, None, samples, handle, decoded.rate)
}

/// One frame of the program after some keys: the keys, the pictures and the screen, timed, and the
/// bytes written.
fn frame(
    app: &mut App,
    graphics: &mut Graphics,
    screen: &mut Terminal<CrosstermBackend<Screen>>,
    keys: &str,
) -> (Duration, usize) {
    let before = SCREEN_WRITTEN.load(Ordering::Relaxed);
    let mut sent = Counter(0);
    let started = Instant::now();
    for key in parse_sequence(keys).expect("key names") {
        app.key(key, Instant::now());
        app.key_released(key, Instant::now());
    }
    graphics.sync(app, &mut sent).expect("writing to memory");
    screen
        .draw(|frame| draw_screen(frame, app, None, Instant::now()))
        .expect("writing to memory");
    graphics.place(app, &mut sent).expect("writing to memory");
    let time = started.elapsed();
    (
        time,
        sent.0 + SCREEN_WRITTEN.load(Ordering::Relaxed) - before,
    )
}

/// Commands that send a PNG for the terminal to keep and answer about.
fn commands(id: u32, png: &[u8]) -> Vec<u8> {
    let encoded = STANDARD.encode(png);
    let mut out = Vec::new();
    let mut chunks = encoded.as_bytes().chunks(CHUNK).peekable();
    let mut first = true;
    while let Some(chunk) = chunks.next() {
        let more = u8::from(chunks.peek().is_some());
        out.extend(b"\x1b_G");
        if first {
            out.extend(format!("a=t,f=100,i={id},q=0,m={more}").as_bytes());
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

/// Wait for the terminal's answer about an image.
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

/// A key and what it cost.
struct Row {
    what: String,
    zoom: u64,
    time: Duration,
    bytes: usize,
    picture: Option<Vec<u8>>,
    decoded: Option<Result<Duration, String>>,
}

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let value = |flag: &str| {
        args.iter()
            .position(|arg| arg == flag)
            .and_then(|at| args.get(at + 1))
            .cloned()
    };
    let decoded = match value("--file") {
        Some(path) => decode_sample(&PathBuf::from(path)).expect("a sample file"),
        None => five_minutes(),
    };
    // A character's size as the terminal has it, or the author's screen's.
    let cell = terminal::window_size()
        .ok()
        .filter(|size| size.width > 0 && size.columns > 0)
        .map_or(DEFAULT_CELL, |size| {
            (
                usize::from(size.width / size.columns),
                usize::from(size.height / size.rows.max(1)),
            )
        });
    let mut app = app(&decoded);
    let mut graphics = Graphics::new();
    graphics.use_cell(cell);
    let options = TerminalOptions {
        viewport: Viewport::Fixed(Rect::new(0, 0, SCREEN.0, SCREEN.1)),
    };
    let mut screen = Terminal::with_options(CrosstermBackend::new(Screen), options)?;
    // The page opened and drawn, twice so the cursor's line is placed.
    frame(&mut app, &mut graphics, &mut screen, "Space 0");
    frame(&mut app, &mut graphics, &mut screen, "M");

    let mut rows = Vec::new();
    // Zooming in from the whole sample, a step at a time, on the middle. Each zoom is timed as
    // it's come to: from the next one out, and the whole sample from the next one in.
    frame(&mut app, &mut graphics, &mut screen, "z O");
    loop {
        let what = if rows.is_empty() { "z O" } else { "z i" };
        let back = if rows.is_empty() { "z i" } else { "z o" };
        let mut times = Vec::new();
        let mut bytes = 0;
        for _ in 0..RUNS {
            frame(&mut app, &mut graphics, &mut screen, back);
            let (time, written) = frame(&mut app, &mut graphics, &mut screen, what);
            times.push(time);
            bytes = written;
        }
        let page = &app.sample_page;
        let shown = app.sample_shown().expect("the sample");
        let png = Drawing {
            analysis: &shown.analysis,
            frames: shown
                .frames
                .as_ref()
                .map(|(data, rates)| (data.as_ref(), *rates)),
            points: &shown.entry.edit.points,
            first: page.first_frame(),
            zoom: page.zoom,
            columns: page.columns,
            rows: usize::from(vt_tui::views::sample::WAVE_ROWS),
            cell,
            merged: false,
        }
        .picture()
        .png();
        rows.push(Row {
            what: what.to_owned(),
            zoom: page.zoom,
            time: median(times),
            bytes,
            picture: Some(png),
            decoded: None,
        });
        if app.sample_page.zoom == 1 {
            break;
        }
        frame(&mut app, &mut graphics, &mut screen, "z i");
    }
    // Scrolling and moving the cursor, at a middling zoom.
    frame(&mut app, &mut graphics, &mut screen, "z O 6 z i");
    let zoom = app.sample_page.zoom;
    for (what, keys) in [("C-e", "C-e"), ("l, a line", "l"), ("l, its column", "l")] {
        if what == "l, its column" {
            frame(&mut app, &mut graphics, &mut screen, "z c");
        }
        let mut times = Vec::new();
        let mut bytes = 0;
        for _ in 0..RUNS {
            let (time, written) = frame(&mut app, &mut graphics, &mut screen, keys);
            times.push(time);
            bytes = written;
        }
        rows.push(Row {
            what: what.to_owned(),
            zoom,
            time: median(times),
            bytes,
            picture: None,
            decoded: None,
        });
    }

    // The terminal's part: decoding each zoom's picture.
    if let Ok(mut tty) = OpenOptions::new().read(true).write(true).open("/dev/tty")
        && args.iter().any(|arg| arg == "--terminal")
    {
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
        let mut pending = Vec::new();
        let mut id = 0x00_7C_00;
        for row in &mut rows {
            let Some(png) = &row.picture else { continue };
            let mut trips = Vec::new();
            let mut failed = None;
            for _ in 0..RUNS {
                id += 1;
                let sent = Instant::now();
                tty.write_all(&commands(id, png))?;
                tty.flush()?;
                let reply = answer(&input, &mut pending, id);
                trips.push(sent.elapsed());
                write!(tty, "\x1b_Ga=d,d=I,i={id},q=2\x1b\\")?;
                if let Err(reply) = reply {
                    failed = Some(reply);
                    break;
                }
            }
            row.decoded = Some(match failed {
                Some(reply) => Err(reply),
                None => Ok(median(trips)),
            });
        }
        terminal::disable_raw_mode()?;
    }

    let mut lines = vec![
        format!(
            "{:.0} s, {} channels at {} Hz, on {}x{} characters of {}x{} pixels. Median of {RUNS}.",
            decoded.info().seconds(),
            decoded.channels,
            decoded.rate,
            SCREEN.0,
            SCREEN.1,
            cell.0,
            cell.1
        ),
        format!(
            "{:<15} {:>14} {:>16} {:>12} {:>16} {:>14}",
            "Key", "Frames/column", "Key to screen", "Bytes", "Terminal decode", "Total"
        ),
    ];
    for row in &rows {
        let (decode, total) = match &row.decoded {
            Some(Ok(decode)) => (
                format!("{:.1} ms", ms(*decode)),
                format!("{:.1} ms", ms(*decode + row.time)),
            ),
            Some(Err(reply)) => (reply.chars().take(16).collect(), String::new()),
            None => (String::new(), String::new()),
        };
        lines.push(format!(
            "{:<15} {:>14} {:>13.1} ms {:>12} {:>16} {:>14}",
            row.what,
            row.zoom,
            ms(row.time),
            row.bytes,
            decode,
            total
        ));
    }
    let table = lines.join("\n");
    match value("--out") {
        Some(path) => std::fs::write(path, format!("{table}\n"))?,
        None => println!("{table}"),
    }
    Ok(())
}
