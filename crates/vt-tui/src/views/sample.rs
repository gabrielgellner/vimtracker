//! The sample page: a header, a ruler with the points and the time, the waveform, an overview of
//! the whole sample with the view shown on it, the entry's settings, and where the cursor is.
//!
//! It's laid out for the author's screen, kitty full screen on a MacBook, 163 characters by 42. A
//! wider window gives the waveform its width, and a smaller one shows part of the page, following
//! the cursor across, rather than squeezing it.

use std::time::Instant;

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use crate::app::App;
use crate::input::mode::Mode;
use crate::kitty;
use crate::sample_page::{SamplePage, Shown};
use crate::sample_wave::{END_COLOR, LOOP_COLOR, START_COLOR};
use crate::theme;
use crate::views::help::POPUP_DELAY;

/// The waveform's rows: 14 for each channel of a stereo sample, left above right, or 28 for a
/// mono one.
pub const WAVE_ROWS: u16 = 28;
/// The rows above the waveform: the header, a gap and the ruler.
pub const WAVE_TOP: u16 = 3;
/// The page's size on the author's screen. A smaller window shows part of it.
pub const PAGE_WIDTH: u16 = 163;
pub const PAGE_HEIGHT: u16 = WAVE_TOP + WAVE_ROWS + 4;
/// The waveform's column either side of it.
const MARGIN: u16 = 1;
/// Where a point is shown in the ruler, above its line.
const POINT_MARK: char = '▾';
/// The fewest columns from one time in the ruler to the next.
const TICK_SPACING: f64 = 12.0;

/// Where the waveform is on screen, for the cursor's line, which the terminal places itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WaveOnScreen {
    /// The screen column of the waveform's first column, which is off the left when the page is
    /// scrolled.
    pub left: i32,
    pub top: u16,
    /// The screen columns of the waveform in view, and how many of its rows are.
    pub columns: std::ops::Range<i32>,
    pub rows: u16,
}

/// The page's size in this much room: at least the author's screen's.
pub fn page_size(area: Rect) -> (u16, u16) {
    (area.width.max(PAGE_WIDTH), area.height.max(PAGE_HEIGHT))
}

/// How many columns the waveform has on a page this wide.
pub fn wave_columns(page_width: u16) -> usize {
    usize::from(page_width.saturating_sub(2 * MARGIN))
}

/// Whether something is drawn over the page, which the cursor's line, placed by the terminal
/// above the text, would show through.
pub fn overlaid(app: &App, now: Instant) -> bool {
    app.picker.is_some()
        || app.guide.is_some()
        || app.shape.is_some()
        || matches!(app.mode, Mode::Help | Mode::Picker | Mode::Guide)
        || (app.mode == Mode::Command && app.command_menu.is_some())
        || !app.popup_rows(now, POPUP_DELAY).is_empty()
}

/// A number of frames as a time: seconds to the thousandth, with minutes from a minute up.
pub fn time(frames: u64, rate: u32) -> String {
    clock(frames as f64 / f64::from(rate.max(1)), 3)
}

/// Seconds as a time with this many decimals, as `1.250 s` or `3:05.250`.
fn clock(seconds: f64, decimals: usize) -> String {
    if seconds < 60.0 {
        format!("{seconds:.decimals$} s")
    } else {
        let minutes = (seconds / 60.0).floor();
        let rest = seconds - minutes * 60.0;
        let width = if decimals == 0 { 2 } else { decimals + 3 };
        format!("{minutes:.0}:{rest:0width$.decimals$}")
    }
}

/// A rate as the header shows it, as `44.1 kHz`.
fn rate_text(rate: u32) -> String {
    let khz = f64::from(rate) / 1000.0;
    if rate.is_multiple_of(1000) {
        format!("{khz:.0} kHz")
    } else {
        format!("{khz:.1} kHz")
    }
}

/// A time step for the ruler that leaves at least [`TICK_SPACING`] columns between its times,
/// and how many decimals it needs: 1, 2 or 5 of a power of ten below 10 seconds, and steps that
/// minutes divide into from there, as a clock's are.
fn tick_step(seconds_a_column: f64) -> (f64, usize) {
    const CLOCK: [f64; 7] = [10.0, 15.0, 30.0, 60.0, 120.0, 300.0, 600.0];
    let wanted = seconds_a_column * TICK_SPACING;
    let mut scale = 1e-6;
    while scale < 10.0 {
        for step in [1.0, 2.0, 5.0] {
            let candidate = step * scale;
            if candidate >= wanted {
                let decimals = (-candidate.log10().floor()).max(0.0) as usize;
                return (candidate, decimals);
            }
        }
        scale *= 10.0;
    }
    let step = CLOCK
        .into_iter()
        .find(|step| *step >= wanted)
        .unwrap_or(CLOCK[CLOCK.len() - 1]);
    (step, 0)
}

pub fn draw(frame: &mut Frame, area: Rect, app: &mut App) {
    let (width, height) = page_size(area);
    app.sample_page.columns = wave_columns(width);
    app.sample_page.on_screen = None;
    let shown = app.sample_page_here();
    let page = Rect::new(0, 0, width, height);
    let mut buffer = Buffer::empty(page);
    buffer.set_style(page, theme::screen());
    match &shown {
        Ok(shown) => draw_page(&mut buffer, page, app, shown),
        Err(why) => {
            let lines = vec![
                Line::styled("Sample", Style::new().add_modifier(Modifier::BOLD)),
                Line::raw(""),
                Line::styled(why.clone(), theme::dim()),
            ];
            Paragraph::new(lines).render(Rect::new(MARGIN, 0, width - MARGIN, 3), &mut buffer);
        }
    }
    // A window smaller than the page shows the part with the cursor's column in it.
    let cursor_column = app
        .sample_page
        .cursor_on_screen()
        .map_or(0, |column| i32::from(MARGIN) + column as i32);
    let scroll_x = if width > area.width {
        (cursor_column - i32::from(area.width) / 2).clamp(0, i32::from(width - area.width))
    } else {
        0
    };
    let target = frame.buffer_mut();
    for y in 0..area.height.min(height) {
        for x in 0..area.width.min(width) {
            let from = Position::new(x + scroll_x as u16, y);
            if let Some(cell) = buffer.cell(from) {
                target[(area.x + x, area.y + y)] = cell.clone();
            }
        }
    }
    if shown.is_ok() {
        let left = i32::from(area.x) + i32::from(MARGIN) - scroll_x;
        let columns = app.sample_page.columns as i32;
        let start = left.max(i32::from(area.x));
        let end = (left + columns).min(i32::from(area.right()));
        let rows = area.height.saturating_sub(WAVE_TOP).min(WAVE_ROWS);
        app.sample_page.on_screen = (rows > 0 && start < end).then_some(WaveOnScreen {
            left,
            top: area.y + WAVE_TOP,
            columns: start..end,
            rows,
        });
    }
}

/// The whole page, into a buffer of its size.
fn draw_page(buffer: &mut Buffer, page: Rect, app: &App, shown: &Shown) {
    let state = &app.sample_page;
    let rate = shown.rate();
    let bold = Style::new().add_modifier(Modifier::BOLD);
    let channels = match shown.analysis.channels() {
        1 => "mono",
        _ => "stereo",
    };
    let frames = match state.zoom {
        1 => "1 frame a column".to_owned(),
        zoom => format!("{zoom} frames a column"),
    };
    let zoom = if state.zoom == state.fit(shown.length()) {
        format!("zoom whole sample, {frames}")
    } else {
        format!("zoom {frames}")
    };
    let header = Line::from(vec![
        Span::styled(format!("Sample {:02X}  ", shown.number), bold),
        Span::styled(shown.entry.title(), bold),
        Span::styled(
            format!(
                "   {}  {}  {channels}  {zoom}",
                time(shown.length(), rate),
                rate_text(rate),
            ),
            theme::dim(),
        ),
    ]);
    let row = |y: u16| Rect::new(MARGIN, y, page.width - MARGIN, 1);
    header.render(row(0), buffer);
    ruler(app, shown).render(row(WAVE_TOP - 1), buffer);

    let columns = state.columns as u16;
    if let Some((wave, overview)) = app.sample_images {
        let style = Style::new().fg(kitty::id_color(wave));
        let cursor = state.cursor_on_screen().filter(|_| !state.cursor_line);
        for (y, line) in kitty::placeholder_lines(state.columns, usize::from(WAVE_ROWS))
            .iter()
            .enumerate()
        {
            let y = WAVE_TOP + y as u16;
            buffer.set_string(MARGIN, y, line, style);
            if let Some(column) = cursor {
                buffer[(MARGIN + column as u16, y)].set_bg(cursor_column());
            }
        }
        let y = WAVE_TOP + WAVE_ROWS;
        let line = &kitty::placeholder_lines(state.columns, 1)[0];
        buffer.set_string(MARGIN, y, line, Style::new().fg(kitty::id_color(overview)));
        // The part of the sample in view, as a band behind the overview.
        let fit = state.fit(shown.length());
        let first = state.first_frame() / fit;
        let last = (state.first_frame() + state.columns as u64 * state.zoom).div_ceil(fit);
        for column in first..last.min(u64::from(columns)) {
            buffer[(MARGIN + column as u16, y)].set_bg(theme::SURFACE0);
        }
    }
    settings(shown).render(row(WAVE_TOP + WAVE_ROWS + 2), buffer);
    cursor_line(state, shown).render(row(WAVE_TOP + WAVE_ROWS + 3), buffer);
}

/// The background of the cursor's column, when it's shown that way: the cursor's own colour,
/// set back so the waveform reads over it.
pub fn cursor_column() -> Color {
    theme::dimmed(theme::ROSEWATER)
}

/// The ruler: each point's mark in its colour, over the column it's in, and the time at round
/// steps where there's room, faint.
fn ruler(app: &App, shown: &Shown) -> Line<'static> {
    let state = &app.sample_page;
    let columns = state.columns;
    let mut cells: Vec<(char, Style)> = vec![(' ', Style::new()); columns];
    let mut taken = vec![false; columns];
    let mut mark = |frame: u64, color: Color| {
        let column = state.column_of(frame).checked_sub(state.column);
        if let Some(column) = column.map(|column| column as usize)
            && column < columns
        {
            cells[column] = (POINT_MARK, Style::new().fg(color));
            taken[column] = true;
        }
    };
    let points = &shown.entry.edit.points;
    if let Some(looped) = points.loop_ {
        mark(looped.start, LOOP_COLOR);
        mark(looped.end, LOOP_COLOR);
    }
    mark(shown.end(), END_COLOR);
    mark(points.start, START_COLOR);
    let rate = f64::from(shown.rate().max(1));
    let (step, decimals) = tick_step(state.zoom as f64 / rate);
    let first = state.first_frame() as f64 / rate;
    let mut tick = (first / step).ceil();
    loop {
        let seconds = tick * step;
        let frame = (seconds * rate).round() as u64;
        let Some(column) = state.column_of(frame).checked_sub(state.column) else {
            tick += 1.0;
            continue;
        };
        let column = column as usize;
        if column >= columns || frame >= shown.length() {
            break;
        }
        let label = format!("╵{}", clock(seconds, decimals).trim_end_matches(" s"));
        // With a column free after it, and whole, or not at all.
        let span = column..column + label.chars().count() + 1;
        if span.end <= columns && !span.clone().any(|at| taken[at]) {
            for (at, c) in span.clone().zip(label.chars()) {
                cells[at] = (c, theme::faint());
            }
            span.for_each(|at| taken[at] = true);
        }
        tick += 1.0;
    }
    Line::from(
        cells
            .into_iter()
            .map(|(c, style)| Span::styled(c.to_string(), style))
            .collect::<Vec<_>>(),
    )
}

/// The entry's points, each named in its colour, so the row is the waveform's key too.
fn settings(shown: &Shown) -> Line<'static> {
    let rate = shown.rate();
    let points = &shown.entry.edit.points;
    let named = |name: &str, color: Color| Span::styled(format!("{name} "), Style::new().fg(color));
    let mut spans = vec![
        named("Start", START_COLOR),
        Span::raw(time(points.start, rate)),
        Span::raw("   "),
        named("End", END_COLOR),
        Span::raw(time(shown.end(), rate)),
        Span::raw("   "),
        named("Loop", LOOP_COLOR),
    ];
    match points.loop_ {
        Some(looped) => {
            spans.push(Span::raw(format!(
                "{} to {}",
                time(looped.start, rate),
                time(looped.end, rate)
            )));
            if looped.crossfade > 0 {
                spans.push(Span::styled(
                    format!(", crossfading {}", time(u64::from(looped.crossfade), rate)),
                    theme::dim(),
                ));
            }
        }
        None => spans.push(Span::raw("off")),
    }
    Line::from(spans)
}

/// Where the cursor is: its time, its frame and its slice.
fn cursor_line(state: &SamplePage, shown: &Shown) -> Line<'static> {
    let (slice, _) = shown.slice_at(state.cursor);
    let style = if state.cursor_line {
        "a line"
    } else {
        "its column"
    };
    Line::from(vec![
        Span::styled("Cursor ", Style::new().fg(theme::ROSEWATER)),
        Span::raw(format!(
            "{}  frame {}  slice {slice:02X}",
            time(state.cursor, shown.rate()),
            state.cursor
        )),
        Span::styled(format!("   drawn as {style}, zc to switch"), theme::faint()),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_read_in_seconds_and_from_a_minute_in_minutes() {
        assert_eq!(time(48_000, 48_000), "1.000 s");
        assert_eq!(time(441, 44_100), "0.010 s");
        assert_eq!(time(48_000 * 185 + 12_000, 48_000), "3:05.250");
        assert_eq!(clock(65.0, 0), "1:05");
        assert_eq!(rate_text(44_100), "44.1 kHz");
        assert_eq!(rate_text(48_000), "48 kHz");
    }

    #[test]
    fn the_rulers_times_are_round_and_far_enough_apart() {
        // A column of 2048 frames at 48 kHz is about 43 ms, so 12 columns are about half a second.
        assert_eq!(tick_step(2048.0 / 48_000.0), (1.0, 0));
        let (step, decimals) = tick_step(1.0 / 48_000.0);
        assert!((step - 0.0005).abs() < 1e-12);
        assert_eq!(decimals, 4);
        // Five minutes across 161 columns: a time each half minute, as a clock has them.
        assert_eq!(tick_step(300.0 / 161.0), (30.0, 0));
        assert_eq!(tick_step(0.7), (10.0, 0));
    }
}
