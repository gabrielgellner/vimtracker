//! The sample page: a header, a ruler with the points and the time, the waveform, an overview of
//! the whole sample with the view shown on it, the entry's settings, the slice's under the cursor,
//! and where the cursor is.
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
use crate::input::mode::{Mode, View};
use crate::kitty;
use crate::placing::{Edge, Point};
use crate::sample_page::{SamplePage, Shown};
use crate::sample_settings::SampleSetting;
use crate::sample_wave::{
    END_COLOR, LOOP_COLOR, MARK_COLOR, POINT_PIXELS, SLICE_COLOR, SLICE_PIXELS, START_COLOR,
};
use crate::theme;
use crate::views::help::POPUP_DELAY;
use crate::views::widgets::toggle;

/// The waveform's rows: 14 for each channel of a stereo sample, left above right, or 28 for a
/// mono one.
pub const WAVE_ROWS: u16 = 28;
/// The rows above the waveform: the header, a gap and the ruler.
pub const WAVE_TOP: u16 = 3;
/// The page's size on the author's screen. A smaller window shows part of it.
pub const PAGE_WIDTH: u16 = 163;
pub const PAGE_HEIGHT: u16 = WAVE_TOP + WAVE_ROWS + 5;
/// The waveform's column either side of it.
const MARGIN: u16 = 1;
/// Where a point is shown in the ruler, above its line, when the terminal isn't placing its mark
/// at the line's pixel: in a render, or while something is drawn over the page.
const POINT_MARK: char = '▾';
/// The fewest columns from one time in the ruler to the next.
const TICK_SPACING: f64 = 12.0;

/// Where the waveform is on screen, for the cursor's line and the ruler's marks, which the
/// terminal places itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WaveOnScreen {
    /// The screen column of the waveform's first column, which is off the left when the page is
    /// scrolled.
    pub left: i32,
    pub top: u16,
    /// The screen columns of the waveform in view, and how many of its rows are.
    pub columns: std::ops::Range<i32>,
    pub rows: u16,
    /// The ruler's marks, in view or not, in the order they're drawn.
    pub marks: Vec<Mark>,
    /// The frames the playheads are at, in view or not.
    pub playheads: Vec<u64>,
}

/// What kind of point a mark in the ruler is for, which gives it its colour and its tip's width.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MarkKind {
    Start,
    End,
    Loop,
    Slice,
}

impl MarkKind {
    pub const ALL: [Self; 4] = [Self::Start, Self::End, Self::Loop, Self::Slice];

    /// Its colour, its line's in the waveform.
    pub fn color(self) -> Color {
        match self {
            Self::Start => START_COLOR,
            Self::End => END_COLOR,
            Self::Loop => LOOP_COLOR,
            Self::Slice => SLICE_COLOR,
        }
    }

    /// How wide its line in the waveform is, in pixels, which is how wide the mark's tip is.
    pub fn line_pixels(self) -> usize {
        match self {
            Self::Slice => SLICE_PIXELS,
            _ => POINT_PIXELS,
        }
    }

    /// Its place in [`Self::ALL`].
    pub fn index(self) -> usize {
        self as usize
    }
}

/// A point's mark in the ruler: a small triangle pointing down, its tip on its line's pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mark {
    pub kind: MarkKind,
    /// Its own number among its kind's marks, which the terminal places it by: 1 for the start,
    /// the end and the loop's start, 2 for the loop's end, and one more than a slice's id, so a
    /// slice's mark keeps its number as slices before it come and go.
    pub placement: u32,
    pub frame: u64,
}

impl Mark {
    /// The pixel across its line starts at, counted from the waveform's left, for characters
    /// this wide, as the waveform draws the line. It's off the left for a frame before the view.
    pub fn line_pixel(&self, state: &SamplePage, cell_width: usize) -> i64 {
        let across = cell_width.max(1) as i128;
        ((i128::from(self.frame) - i128::from(state.first_frame())) * across)
            .div_euclid(i128::from(state.zoom.max(1))) as i64
    }

    /// The pixels across it covers, counted from the waveform's left: its first, and how many.
    /// It reaches [`mark_reach`] either side of its line.
    pub fn pixels(&self, state: &SamplePage, cell_width: usize) -> (i64, usize) {
        let reach = mark_reach(cell_width);
        (
            self.line_pixel(state, cell_width) - reach as i64,
            self.kind.line_pixels() + 2 * reach,
        )
    }
}

/// How far a mark reaches either side of its line, in pixels, for characters this wide: a third
/// of a character, so it's about as wide as `▾`, 12 pixels on the author's screen.
pub fn mark_reach(cell_width: usize) -> usize {
    (cell_width / 3).max(1)
}

/// The ruler's marks: each slice's start, the loop's start and end, the end and the start, in
/// that order, so the start is drawn last, over the rest where they meet, as the waveform draws
/// their lines.
pub fn marks(shown: &Shown) -> Vec<Mark> {
    let points = &shown.entry.edit.points;
    let mark = |kind, placement, frame| Mark {
        kind,
        placement,
        frame,
    };
    let mut marks: Vec<Mark> = points
        .slices
        .iter()
        .map(|slice| mark(MarkKind::Slice, slice.id.saturating_add(1), slice.start))
        .collect();
    if let Some(looped) = points.loop_ {
        marks.push(mark(MarkKind::Loop, 1, looped.start));
        marks.push(mark(MarkKind::Loop, 2, looped.end));
    }
    marks.push(mark(MarkKind::End, 1, shown.end()));
    marks.push(mark(MarkKind::Start, 1, points.start));
    marks
}

/// Whether the terminal places the ruler's marks at their lines' pixels, rather than the ruler
/// drawing each as `▾` over its column: when the pictures are drawn and nothing is drawn over the
/// page, which a placed image would show through.
pub fn marks_placed(app: &App, now: Instant) -> bool {
    app.view == View::Sample
        && app.sample_images.is_some()
        && app.sample_page.cell_width.is_some()
        && !overlaid(app, now)
}

/// Whether the terminal places the cursor and the playheads as thin lines at their frames' pixels,
/// rather than the page drawing their columns' backgrounds: when the marks are placed and the
/// cursor is a line, as `zc` has it, so the playheads follow the cursor's style.
pub fn lines_placed(app: &App, now: Instant) -> bool {
    marks_placed(app, now) && app.sample_page.cursor_line
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
            marks: shown.as_ref().map(marks).unwrap_or_default(),
            playheads: shown
                .as_ref()
                .map(|shown| app.sample_playheads(shown))
                .unwrap_or_default(),
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
        // The selection, as the selection's grey behind the columns it covers.
        let selected: Vec<u64> = state
            .selection()
            .map(|frames| {
                let first = state.column_of(frames.start).max(state.column);
                let last = state
                    .column_of(frames.end - 1)
                    .min(state.column + u64::from(columns.saturating_sub(1)));
                (first..=last).map(|column| column - state.column).collect()
            })
            .unwrap_or_default();
        // The playheads as their columns, where the terminal isn't placing them as lines.
        let playheads: Vec<usize> = if lines_placed(app, Instant::now()) {
            Vec::new()
        } else {
            app.sample_playheads(shown)
                .into_iter()
                .filter_map(|frame| state.on_screen_column(frame))
                .collect()
        };
        for (y, line) in kitty::placeholder_lines(state.columns, usize::from(WAVE_ROWS))
            .iter()
            .enumerate()
        {
            let y = WAVE_TOP + y as u16;
            buffer.set_string(MARGIN, y, line, style);
            for column in &selected {
                buffer[(MARGIN + *column as u16, y)].set_bg(theme::SURFACE1);
            }
            for column in &playheads {
                buffer[(MARGIN + *column as u16, y)].set_bg(playhead_column());
            }
            if let Some(column) = cursor {
                let color = if state.setting.is_some() && app.mode != Mode::Keys {
                    theme::dimmed(cursor_column())
                } else {
                    cursor_column()
                };
                buffer[(MARGIN + column as u16, y)].set_bg(color);
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
    settings(state, shown).render(row(WAVE_TOP + WAVE_ROWS + 2), buffer);
    slice_settings(state, shown).render(row(WAVE_TOP + WAVE_ROWS + 3), buffer);
    cursor_line(app, shown).render(row(WAVE_TOP + WAVE_ROWS + 4), buffer);
}

/// The background of the cursor's column, when it's shown that way: the cursor's own colour,
/// set back so the waveform reads over it.
pub fn cursor_column() -> Color {
    theme::dimmed(theme::ROSEWATER)
}

/// The background of a playhead's column, where it isn't a line: the playhead's green, set back
/// as the cursor's column is.
pub fn playhead_column() -> Color {
    theme::dimmed(theme::GREEN)
}

/// The ruler: each point's mark in its colour, over the column it's in, each slice's number in
/// hex before its mark where there's room, as Vim numbers lines, the letters naming it after its
/// mark, as `03▾a`, and the time at round steps
/// where there's room after that, faint. With the pictures drawn, the terminal places each mark
/// at its line's pixels, so the ruler leaves the columns the mark covers for it, and draws it as
/// `▾` itself only while something is drawn over the page.
fn ruler(app: &App, shown: &Shown) -> Line<'static> {
    let state = &app.sample_page;
    let columns = state.columns;
    let placed = marks_placed(app, Instant::now());
    let mut cells: Vec<(char, Style)> = vec![(' ', Style::new()); columns];
    let mut taken = vec![false; columns];
    let on_screen = |frame: u64| {
        let column = state.column_of(frame).checked_sub(state.column)? as usize;
        (column < columns).then_some(column)
    };
    // The columns a mark covers, from its first to its last: with the pictures drawn, those its
    // pixels are in, which can be the column either side of its line's, and otherwise its line's.
    let covers = |mark: &Mark, column: usize| match state.cell_width {
        Some(across) => {
            let (first, width) = mark.pixels(state, across);
            let across = across as i64;
            let last = first + width as i64 - 1;
            (
                first.div_euclid(across).max(0) as usize,
                (last.div_euclid(across).max(0) as usize).min(columns - 1),
            )
        }
        None => (column, column),
    };
    let marks = marks(shown);
    for mark in &marks {
        if let Some(column) = on_screen(mark.frame) {
            let (first, last) = covers(mark, column);
            taken[first..=last].fill(true);
            if !placed {
                cells[column] = (POINT_MARK, Style::new().fg(mark.kind.color()));
            }
        }
    }
    // Each slice's number, in the two columns before its mark, or after it at the ruler's left
    // edge, if they're free.
    let slices: Vec<&Mark> = marks
        .iter()
        .filter(|mark| mark.kind == MarkKind::Slice)
        .collect();
    // Where the letters naming each slice can start, after its mark or a number after it.
    let mut after = vec![None; slices.len()];
    for (number, mark) in slices.iter().enumerate() {
        let Some(column) = on_screen(mark.frame) else {
            continue;
        };
        let (first, last) = covers(mark, column);
        after[number] = Some(last + 1);
        let from = first.checked_sub(2).unwrap_or(last + 1);
        if from + 2 > columns || taken[from..from + 2].iter().any(|taken| *taken) {
            continue;
        }
        let style = Style::new().fg(SLICE_COLOR);
        for (at, c) in (from..from + 2).zip(format!("{number:02X}").chars()) {
            cells[at] = (c, style);
            taken[at] = true;
        }
        if from > last {
            after[number] = Some(from + 2);
        }
    }
    // The letters naming each slice, whole after its mark if they're free, once every number is
    // in, so a slice's number before its mark wins over the letters of the slice before.
    let points = &shown.entry.edit.points;
    for (number, from) in after.into_iter().enumerate() {
        let letters = points.marks_of(number);
        let Some(from) = from.filter(|_| !letters.is_empty()) else {
            continue;
        };
        let span = from..from + letters.len();
        if span.end > columns || span.clone().any(|at| taken[at]) {
            continue;
        }
        let style = Style::new().fg(MARK_COLOR);
        for (at, letter) in span.zip(letters) {
            cells[at] = (letter, style);
            taken[at] = true;
        }
    }
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

/// A setting's value on the rows: in the cursor's colours while the focus is on it, the text's
/// if it's the slice's own, and dimmer if it's the entry's, which the slice plays as. `Snap` and
/// `Reverse`, which are off or on, are a toggle, in its own colour where it's the slice's own.
fn setting_span(state: &SamplePage, shown: &Shown, setting: SampleSetting) -> Span<'static> {
    let (value, own) = shown.setting_value(setting, state.cursor);
    let toggle =
        matches!(setting, SampleSetting::Snap | SampleSetting::Reverse).then_some(value != 0);
    let text = match toggle {
        Some(on) => toggle::text(on),
        None => setting.text(value, shown.rate()),
    };
    let style = if state.setting == Some(setting) {
        theme::cursor(Mode::Normal)
    } else if !own {
        theme::dim()
    } else {
        toggle.map_or_else(Style::new, toggle::style)
    };
    Span::styled(text, style)
}

/// The entry's points, each named in its colour, so the row is the waveform's key too, and
/// whether points placed snap to zero crossings.
fn settings(state: &SamplePage, shown: &Shown) -> Line<'static> {
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
    spans.push(Span::raw("   "));
    spans.push(named("Slices", SLICE_COLOR));
    spans.push(Span::raw(match points.slices.len() {
        0 => "none".to_owned(),
        count => count.to_string(),
    }));
    let marks = points.marks_in_order();
    if !marks.is_empty() {
        spans.push(Span::raw("   "));
        spans.push(named("Marks", MARK_COLOR));
        let letters: String = marks.iter().map(|(letter, _)| *letter).collect();
        spans.push(Span::raw(letters));
    }
    spans.push(Span::raw("   "));
    spans.push(Span::raw("Snap "));
    spans.push(setting_span(state, shown, SampleSetting::Snap));
    Line::from(spans)
}

/// How the slice under the cursor plays, its own settings in the text's colour and those it plays
/// as the entry does dimmer, or the entry's own with no slices.
fn slice_settings(state: &SamplePage, shown: &Shown) -> Line<'static> {
    let mut spans = match shown.settings_slice(state.cursor) {
        Some(slice) => {
            let mut spans = vec![Span::styled(
                format!("Slice {slice:02X}"),
                Style::new().fg(SLICE_COLOR),
            )];
            let letters: Vec<String> = shown
                .entry
                .edit
                .points
                .marks_of(slice)
                .into_iter()
                .map(|letter| format!("'{letter}"))
                .collect();
            if !letters.is_empty() {
                spans.push(Span::styled(
                    format!(" {}", letters.join(" ")),
                    Style::new().fg(MARK_COLOR),
                ));
            }
            spans
        }
        None => vec![Span::styled("Whole sample", Style::new().fg(SLICE_COLOR))],
    };
    for setting in shown.settings() {
        if setting == SampleSetting::Snap {
            continue;
        }
        spans.push(Span::raw(format!("   {} ", setting.name())));
        spans.push(setting_span(state, shown, setting));
    }
    Line::from(spans)
}

/// Where the cursor is: its time, its frame and its slice, the point under it, which the nudges
/// and `x` act on, and the selection while there's one.
fn cursor_line(app: &App, shown: &Shown) -> Line<'static> {
    let state = &app.sample_page;
    let rate = shown.rate();
    let (slice, _) = shown.slice_at(state.cursor);
    let style = if state.cursor_line {
        "a line"
    } else {
        "its column"
    };
    let mut spans = vec![
        Span::styled("Cursor ", Style::new().fg(theme::ROSEWATER)),
        Span::raw(format!(
            "{}  frame {}  ",
            time(state.cursor, rate),
            state.cursor
        )),
        Span::styled(format!("slice {slice:02X}"), Style::new().fg(SLICE_COLOR)),
    ];
    if let Some(point) = app.point_here(shown) {
        let (name, color) = match point {
            Point::Slice(number) => (format!("slice {number:02X}'s start"), SLICE_COLOR),
            Point::Edge(Edge::Start) => ("the start".to_owned(), START_COLOR),
            Point::Edge(Edge::End) => ("the end".to_owned(), END_COLOR),
            Point::Edge(Edge::LoopStart) => ("the loop's start".to_owned(), LOOP_COLOR),
            Point::Edge(Edge::LoopEnd) => ("the loop's end".to_owned(), LOOP_COLOR),
        };
        spans.push(Span::raw("  on "));
        spans.push(Span::styled(name, Style::new().fg(color)));
    }
    if let Some(selected) = state.selection() {
        spans.push(Span::raw(format!(
            "   Selection {} to {}, {}",
            time(selected.start, rate),
            time(selected.end, rate),
            time(selected.end - selected.start, rate)
        )));
    }
    spans.push(Span::styled(
        format!("   drawn as {style}, zc to switch"),
        theme::faint(),
    ));
    Line::from(spans)
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
