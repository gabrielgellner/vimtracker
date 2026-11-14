//! For the widgets' tests: what a buffer draws, as lines a test can pin, its look as well as its
//! letters, so a change to how a widget is built can be checked to draw the same (the UI plan's
//! U2, #739).
//!
//! Each row is two lines: its text, an image's placeholder as `▒`, and under it a letter for each
//! cell naming its style, `.` for none. After the rows, each letter's style: its colours, by the
//! theme's names where they have one, and its modifiers. A failing pin prints what was drawn in
//! the same form, ready to paste in.

use std::sync::Arc;
use std::time::Instant;

use ratatui::buffer::Buffer;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::Widget;
use vt_engine::engine::Engine;
use vt_engine::link::{Realtime, connect};
use vt_engine::published::Published;
use vt_model::samples::SamplePool;
use vt_model::song::Song;

use crate::app::App;
use crate::input::keys::Key;
use crate::kitty;
use crate::theme;

/// The letters that name the styles in a row's key, in the order they're first met.
const LETTERS: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";

/// A widget drawn into an empty buffer this big.
pub fn drawn(widget: impl Widget, width: u16, height: u16) -> Buffer {
    let area = Rect::new(0, 0, width, height);
    let mut buffer = Buffer::empty(area);
    widget.render(area, &mut buffer);
    buffer
}

/// Part of a buffer, as a buffer of its own.
pub fn cut(buffer: &Buffer, area: Rect) -> Buffer {
    let mut part = Buffer::empty(Rect::new(0, 0, area.width, area.height));
    for y in 0..area.height {
        for x in 0..area.width {
            part[(x, y)] = buffer[(area.x + x, area.y + y)].clone();
        }
    }
    part
}

/// A buffer as lines: each row's text and its style key, then what each letter in the keys means.
pub fn lines(buffer: &Buffer) -> Vec<String> {
    let area = buffer.area;
    let mut styles: Vec<Style> = Vec::new();
    let mut lines = Vec::new();
    for y in area.top()..area.bottom() {
        let mut text = String::new();
        let mut key = String::new();
        for x in area.left()..area.right() {
            let cell = &buffer[(x, y)];
            let symbol = cell.symbol();
            if symbol.starts_with(kitty::PLACEHOLDER) {
                text.push('▒');
            } else {
                text.push_str(symbol);
            }
            let style = Style::new()
                .fg(cell.fg)
                .bg(cell.bg)
                .underline_color(cell.underline_color)
                .add_modifier(cell.modifier);
            let plain = [cell.fg, cell.bg, cell.underline_color] == [Color::Reset; 3];
            if plain && cell.modifier.is_empty() {
                key.push('.');
                continue;
            }
            let at = styles
                .iter()
                .position(|seen| *seen == style)
                .unwrap_or_else(|| {
                    styles.push(style);
                    styles.len() - 1
                });
            key.push(LETTERS.chars().nth(at).expect("fewer styles than letters"));
        }
        lines.push(text);
        lines.push(key);
    }
    for (style, letter) in styles.iter().zip(LETTERS.chars()) {
        lines.push(format!("{letter} {}", described(*style)));
    }
    lines
}

/// A style in words: its colours and modifiers, as `fg MAUVE bg BASE +BOLD`.
fn described(style: Style) -> String {
    let mut words = Vec::new();
    for (name, colour) in [
        ("fg", style.fg),
        ("bg", style.bg),
        ("ul", style.underline_color),
    ] {
        if let Some(colour) = colour.filter(|colour| *colour != Color::Reset) {
            words.push(format!("{name} {}", colour_name(colour)));
        }
    }
    for (name, _) in style.add_modifier.iter_names() {
        words.push(format!("+{name}"));
    }
    words.join(" ")
}

/// A colour by its name in the theme, or as `#rrggbb`.
fn colour_name(colour: Color) -> String {
    let named = [
        (theme::ROSEWATER, "ROSEWATER"),
        (theme::PINK, "PINK"),
        (theme::MAUVE, "MAUVE"),
        (theme::MAROON, "MAROON"),
        (theme::RED, "RED"),
        (theme::PEACH, "PEACH"),
        (theme::YELLOW, "YELLOW"),
        (theme::GREEN, "GREEN"),
        (theme::TEAL, "TEAL"),
        (theme::SKY, "SKY"),
        (theme::BLUE, "BLUE"),
        (theme::LAVENDER, "LAVENDER"),
        (theme::TEXT, "TEXT"),
        (theme::SUBTEXT1, "SUBTEXT1"),
        (theme::OVERLAY2, "OVERLAY2"),
        (theme::OVERLAY0, "OVERLAY0"),
        (theme::SURFACE1, "SURFACE1"),
        (theme::SURFACE0, "SURFACE0"),
        (theme::BASE, "BASE"),
        (theme::MANTLE, "MANTLE"),
    ];
    match named.iter().find(|(named, _)| *named == colour) {
        Some((_, name)) => (*name).to_owned(),
        None => match colour {
            Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
            other => format!("{other:?}"),
        },
    }
}

/// That a buffer draws these lines, as [`lines`] gives them. If it doesn't, what it draws is
/// printed in the same form, to be checked by eye and pasted in.
#[track_caller]
pub fn assert_drawn(buffer: &Buffer, expected: &[&str]) {
    let drawn = lines(buffer);
    if drawn != expected {
        let pasted: String = drawn
            .iter()
            .map(|line| format!("    {line:?},\n"))
            .collect();
        panic!("drawn:\n[\n{pasted}]\nexpected:\n{expected:#?}");
    }
}

/// An app with a song of two tracks and a real engine, as the app's tests start with.
pub fn app() -> (App, Realtime) {
    app_with(Song::new(2))
}

/// An app with this song and a real engine.
pub fn app_with(song: Song) -> (App, Realtime) {
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, realtime) = connect(Engine::new(48_000, version));
    (App::new(song, None, samples, handle, 48_000), realtime)
}

/// Each key pressed in turn, a space being the leader.
pub fn press(app: &mut App, keys: &str) {
    let now = Instant::now();
    for c in keys.chars() {
        app.key(Key::char(c), now);
    }
}

/// A key that isn't a character, such as Enter or Esc.
pub fn special(app: &mut App, code: KeyCode) {
    app.key(Key::new(code), Instant::now());
}
