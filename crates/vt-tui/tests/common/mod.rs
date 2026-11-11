//! What the app's test files share: an app to drive, keys to press, and ways to read the
//! screen. Each file uses some of them.

#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Instant;

use ratatui::crossterm::event::KeyCode;
use ratatui::style::Color;
use vt_engine::engine::Engine;
use vt_engine::link::{Realtime, connect};
use vt_engine::published::Published;
use vt_model::samples::SamplePool;
use vt_model::song::{Phrase, Song};
use vt_model::step::{Note, Step};
use vt_tui::app::App;
use vt_tui::input::keys::{Key, parse_sequence};

pub fn app() -> (App, Realtime) {
    let song = Song::new(2);
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, realtime) = connect(Engine::new(48_000, version));
    (App::new(song, None, samples, handle, 48_000), realtime)
}

pub fn press(app: &mut App, keys: &str) {
    let now = Instant::now();
    for c in keys.chars() {
        app.key(Key::char(c), now);
    }
}

/// An item as a list shows it: its name, and its meaning beside it.
pub fn listed(item: &vt_tui::input::picker::Item) -> String {
    if item.meaning.is_empty() {
        item.label.clone()
    } else {
        format!("{}  {}", item.label, item.meaning)
    }
}

pub fn special(app: &mut App, code: KeyCode) {
    app.key(Key::new(code), Instant::now());
}

pub fn command(app: &mut App, line: &str) {
    press(app, ":");
    press(app, line);
    special(app, KeyCode::Enter);
}

pub fn folder() -> PathBuf {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let path = std::env::temp_dir().join(format!(
        "vt-tui-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&path);
    path
}

/// Run the audio side for a block.
pub fn block(realtime: &mut Realtime, app: &mut App) {
    let mut out = vec![0.0; 512];
    realtime.process(&mut out);
    app.tick();
}

/// A one-frame WAV, whose value comes from its name, so files of different names are different
/// files to the bank, which knows a file by its content.
pub fn wav(path: &std::path::Path) {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 48_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let name = path.file_name().unwrap().to_string_lossy();
    let value = name.bytes().fold(1000_i16, |value, byte| {
        value.wrapping_mul(31).wrapping_add(i16::from(byte))
    });
    let mut writer = hound::WavWriter::create(path, spec).unwrap();
    writer.write_sample(value).unwrap();
    writer.finalize().unwrap();
}

pub fn screen(app: &App) -> ratatui::buffer::Buffer {
    let backend = ratatui::backend::TestBackend::new(80, 24);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::phrase::draw(frame, frame.area(), app))
        .unwrap();
    terminal.backend().buffer().clone()
}

/// A kit file beside its sounds, returning the kit's path.
pub fn kit_file() -> PathBuf {
    let root = folder();
    let sounds = root.join("drums");
    std::fs::create_dir_all(&sounds).unwrap();
    wav(&sounds.join("kick.wav"));
    wav(&sounds.join("snare.wav"));
    wav(&sounds.join("hat.wav"));
    let kit = root.join("909.ron");
    std::fs::write(
        &kit,
        r#"(sounds: [
            (path: "drums/snare.wav"),
            (path: "drums/kick.wav"),
            (path: "drums/hat.wav"),
        ])"#,
    )
    .unwrap();
    kit
}

/// The file a track's sampler plays, through the bank.
pub fn file_of(song: &Song, track: usize) -> Option<String> {
    let number = song.tracks.get(track)?.sample();
    let entry = song.bank.get(number)?;
    Some(entry.source()?.path.clone())
}

pub fn sample_of(app: &App, track: usize) -> Option<String> {
    file_of(app.song(), track)
}

/// Choose a machine with `K` on the Sound page's Machine row, by typing its name.
pub fn choose_machine(app: &mut App, name: &str) {
    press(app, " 4gg");
    press(app, "K");
    press(app, name);
    special(app, KeyCode::Enter);
}

/// Run the audio side long enough to act on what was sent, and return the loudest sample.
pub fn loudest(realtime: &mut Realtime, frames: usize) -> f32 {
    let mut out = vec![0.0_f32; frames * 2];
    realtime.process(&mut out);
    out.iter().fold(0.0, |a, b| a.max(b.abs()))
}

pub fn with_a_sound(app: &mut App) {
    let dir = folder();
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("tone.wav");
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 48_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(&path, spec).unwrap();
    for _ in 0..48_000 {
        writer.write_sample(16_000_i16).unwrap();
    }
    writer.finalize().unwrap();
    command(app, &format!("sample {}", path.display()));
}

/// A selected 4-step track beside a 2-step track and an 8-step track, whose steps are numbered by
/// their notes.
pub fn tracks_of_different_lengths() -> (App, Realtime) {
    let mut song = Song::new(3);
    for (id, length) in [(0, 4), (1, 2), (2, 8)] {
        let mut phrase = Phrase::new(length);
        for step in 0..length {
            *phrase.step_mut(step).unwrap() = Step {
                note: Note::On(60 + step as u8),
                ..Step::default()
            };
        }
        song.tracks[id].phrases.insert(0, Arc::new(phrase));
    }
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, realtime) = connect(Engine::new(48_000, version));
    (App::new(song, None, samples, handle, 48_000), realtime)
}

/// From `Env 1`'s destination, at the top of the Mod page, to an LFO's: across to `LFO 1`'s on
/// the right, and down that column a device at a time.
pub fn to_lfo(lfo: usize) -> String {
    format!("l{}", "}".repeat(lfo))
}

/// A device's title on the track page, by its name, from its frame's corner to the other, as
/// `╭─ LFO 1     Cut ───…╮` with its switch after its name, drawn as wide as the author's screen has room for.
pub fn device_title(app: &App, identity: &str) -> String {
    let (width, height) = (150, 40);
    let backend = ratatui::backend::TestBackend::new(width, height);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::track::draw(frame, frame.area(), app))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let start = format!("╭─ {identity} ");
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .find_map(|line| {
            let at = line.find(&start)?;
            let end = line[at..].find('╮')? + at;
            Some(line[at..end + '╮'.len_utf8()].to_owned())
        })
        .unwrap_or_else(|| panic!("{identity}'s title"))
}

/// From `Env 1`'s destination, at the top of the Mod page, to another envelope's: down the left
/// column a device at a time.
pub fn to_envelope(envelope: usize) -> String {
    "}".repeat(envelope)
}

/// Start typing a value, type it and enter it.
pub fn typed(app: &mut App, text: &str) {
    press(app, "i");
    press(app, text);
    special(app, KeyCode::Enter);
}

/// Everything the track page draws, a line at a time.
pub fn track_page_text(app: &App) -> Vec<String> {
    let backend = ratatui::backend::TestBackend::new(80, 16);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::track::draw(frame, frame.area(), app))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    (0..16)
        .map(|y| (0..80).map(|x| buffer[(x, y)].symbol()).collect())
        .collect()
}

pub fn alt(app: &mut App, name: &str) {
    app.key(parse_sequence(name).unwrap()[0], Instant::now());
}

/// A key with Cmd, pressed and let go as the terminal sends it, through the check that drops
/// unbound Cmd keys.
pub fn with_cmd(app: &mut App, name: &str) {
    use ratatui::crossterm::event::KeyEventKind;
    let key = parse_sequence(name).unwrap()[0];
    let now = Instant::now();
    app.key_event(key, KeyEventKind::Press, now);
    app.key_event(key, KeyEventKind::Release, now);
}

/// The FX page's lines, drawn as wide and tall as the page is beside the main meter at 163x42.
pub fn fx_page_text(app: &App) -> Vec<String> {
    let backend = ratatui::backend::TestBackend::new(150, 40);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::track::draw(frame, frame.area(), app))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    (0..40)
        .map(|y| (0..150).map(|x| buffer[(x, y)].symbol()).collect())
        .collect()
}

/// A slot's title as the FX page writes it on its frame's top: its digit, its bypass's toggle, on
/// or off, two cells with the plain space it's drawn over, or on an empty slot the room for one,
/// then what's after it.
pub fn slot_title(slot: usize, switch: Option<bool>, controls: &str) -> String {
    let switch = match switch {
        Some(on) => format!("{} ", vt_tui::views::widgets::toggle::glyph(on)),
        None => "  ".to_owned(),
    };
    format!("╭─ {}  {switch} {controls}", slot + 1)
}

/// An envelope's or an LFO's title, as `device_title` gives it: its name, its switch on or off,
/// two cells with the plain space it's drawn over, then what follows, such as its destination.
pub fn modulator_title(identity: &str, on: bool, controls: &str) -> String {
    let switch = vt_tui::views::widgets::toggle::glyph(on);
    format!("╭─ {identity}  {switch}  {controls}")
}

/// The chain the FX page's design shows: Glue, EQ3, an Overdrive and a Delay, with the cursor back
/// on the first slot's effect.
pub fn four_devices(app: &mut App) {
    for (effect, kind) in [
        ("Comp", "=="),
        ("EQ3", ""),
        ("Distort", "--"),
        ("Delay", ""),
    ] {
        press(app, "K");
        press(app, effect);
        special(app, KeyCode::Enter);
        press(app, &format!("l{kind}h}}"));
    }
    press(app, "gg");
}

pub fn on_fx_page() -> (App, vt_engine::link::Realtime) {
    let (mut app, realtime) = app();
    press(&mut app, " 4LL");
    (app, realtime)
}

pub fn status_line(app: &App) -> String {
    let backend = ratatui::backend::TestBackend::new(160, 2);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::status::draw(frame, frame.area(), app))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    (0..160).map(|x| buffer[(x, 0)].symbol()).collect()
}

/// A PNG's chunk, by its name.
pub fn png_chunk(png: &[u8], name: &[u8; 4]) -> Vec<u8> {
    let mut at = 8;
    while at + 8 <= png.len() {
        let length = u32::from_be_bytes(png[at..at + 4].try_into().unwrap()) as usize;
        if &png[at + 4..at + 8] == name {
            return png[at + 8..at + 8 + length].to_vec();
        }
        at += 12 + length;
    }
    panic!("no {name:?} chunk")
}

/// A PNG's width and height in pixels.
pub fn png_size(png: &[u8]) -> (u32, u32) {
    let header = png_chunk(png, b"IHDR");
    let number = |at: usize| u32::from_be_bytes(header[at..at + 4].try_into().unwrap());
    (number(0), number(4))
}

/// The track page drawn as wide and tall as the page is beside the main meter at 163x42.
pub fn track_page_buffer(app: &App) -> ratatui::buffer::Buffer {
    let backend = ratatui::backend::TestBackend::new(150, 40);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::track::draw(frame, frame.area(), app))
        .unwrap();
    terminal.backend().buffer().clone()
}

/// Where two drawings of the same page differ, which should be only toggles flipped: every other
/// cell's character is the same, so nothing has moved, and every toggle in either is its glyph
/// then a plain space in its colours, two cells, which the terminal draws the glyph over. Gives
/// the cells that flipped.
pub fn toggles_flipped(
    before: &ratatui::buffer::Buffer,
    after: &ratatui::buffer::Buffer,
) -> Vec<(u16, u16)> {
    use vt_tui::glyphs::{SWITCH_OFF, SWITCH_ON};
    let switch = |symbol: &str| {
        [SWITCH_ON, SWITCH_OFF]
            .iter()
            .any(|glyph| symbol == glyph.to_string())
    };
    assert_eq!(before.area, after.area);
    let area = before.area;
    let mut flipped = Vec::new();
    for y in 0..area.height {
        for x in 0..area.width {
            for buffer in [before, after] {
                if switch(buffer[(x, y)].symbol()) {
                    let space = &buffer[(x + 1, y)];
                    assert_eq!(
                        space.symbol(),
                        " ",
                        "the second cell of the toggle at {x}, {y}"
                    );
                    assert_eq!(space.fg, buffer[(x, y)].fg, "at {x}, {y}");
                    assert_eq!(space.bg, buffer[(x, y)].bg, "at {x}, {y}");
                }
            }
            let (was, now) = (before[(x, y)].symbol(), after[(x, y)].symbol());
            if was != now {
                assert!(
                    switch(was) && switch(now),
                    "{was:?} became {now:?} at {x}, {y}, which isn't a toggle flipping"
                );
                flipped.push((x, y));
            }
        }
    }
    flipped
}

/// A cell's glyph and colours.
pub fn glyph_at(buffer: &ratatui::buffer::Buffer, at: (u16, u16)) -> (char, Color, Color) {
    let cell = &buffer[at];
    (cell.symbol().chars().next().unwrap(), cell.fg, cell.bg)
}

/// A view drawn 160 columns wide and 24 high.
pub fn drawn(
    app: &App,
    draw: fn(&mut ratatui::Frame, ratatui::layout::Rect, &App),
) -> ratatui::buffer::Buffer {
    let backend = ratatui::backend::TestBackend::new(160, 24);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| draw(frame, frame.area(), app))
        .unwrap();
    terminal.backend().buffer().clone()
}

/// The whole screen at the author's size, a line at a time with a character for each cell, an
/// image's placeholder being `▒`, so a column is a count of characters.
pub fn screen_cells(app: &mut App) -> Vec<String> {
    let (width, height) = (163, 42);
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| vt_tui::run::draw_screen(frame, app, None, Instant::now()))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| {
                    let symbol = buffer[(x, y)].symbol();
                    match symbol.starts_with('\u{10EEEE}') {
                        true => "▒".to_owned(),
                        false => symbol.to_owned(),
                    }
                })
                .collect()
        })
        .collect()
}

/// Where a device is on the screen, by its name: the row of its title, and the column of its
/// frame's left edge.
pub fn device_at(lines: &[String], identity: &str) -> (usize, usize) {
    let title = format!("╭─ {identity} ");
    for (row, line) in lines.iter().enumerate() {
        if let Some(at) = line.find(&title) {
            return (row, line[..at].chars().count());
        }
    }
    panic!("{identity} in\n{}", lines.join("\n"));
}

/// A line from a column on, counted in characters.
pub fn from_column(line: &str, column: usize) -> String {
    line.chars().skip(column).collect()
}
