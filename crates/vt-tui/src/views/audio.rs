//! The audio page: the output playing, its rate and buffer, how it's keeping up, whether
//! vimtracker drives the Roto-Control, and whether the mouse wheel turns the cursor's setting.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::App;
use crate::audio_page::{AudioRow, buffer_ms};
use crate::theme;
use crate::views::system;
use crate::views::widgets::setting;
use crate::views::widgets::table::{Columns, blank_row, take_top};
use crate::views::widgets::toggle;

/// Room for a row's name.
const NAME_WIDTH: u16 = 12;

/// A row's value as the page shows it.
fn value(app: &App, row: AudioRow) -> String {
    let audio = &app.audio;
    match row {
        AudioRow::Device if audio.device.is_empty() => "None".to_owned(),
        AudioRow::Device => audio.device.clone(),
        AudioRow::Rate => format!("{} Hz", app.rate),
        AudioRow::Buffer => {
            let own = if audio.choice.buffer.is_none() {
                ", the device's own"
            } else {
                ""
            };
            match audio.buffer.or(audio.choice.buffer) {
                Some(frames) => format!(
                    "{frames} frames, {:.1} ms{own}",
                    buffer_ms(frames, app.rate)
                ),
                None => "The device's own".to_owned(),
            }
        }
        AudioRow::Roto => app.roto.connection.describe(),
        AudioRow::Wheel if app.wheel.setting.on => {
            "Scrolling turns the setting under the cursor. Shift-drag selects text".to_owned()
        }
        AudioRow::Wheel => "Off: the terminal keeps its mouse".to_owned(),
    }
}

/// An off/on row's value, the Roto's or the wheel's: its switch in its own colour, or the
/// cursor's, then what it says, dim while it's off.
fn switch_value(app: &App, row: AudioRow, here: bool) -> Line<'static> {
    let on = match row {
        AudioRow::Wheel => app.wheel.setting.on,
        _ => app.roto.setting.on,
    };
    let switch = if here {
        Span::styled(toggle::text(on), setting::style(app, true, false))
    } else {
        toggle::span(on)
    };
    let style = if on { Style::new() } else { theme::dim() };
    Line::from(vec![
        switch,
        Span::raw(" "),
        Span::styled(value(app, row), style),
    ])
}

pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let bold = Style::new().add_modifier(Modifier::BOLD);
    let mut rest = area;
    frame.render_widget(
        Paragraph::new(Line::styled("Audio", bold)),
        take_top(&mut rest, 2),
    );
    // A row's name and then its value, which has the rest of the room.
    let columns = Columns::new([NAME_WIDTH, 0]).open_ended();
    let named = |name: &'static str, value: Line<'static>| {
        let mut cells = columns.row();
        cells.cell(Line::styled(name, theme::dim())).cell(value);
        cells.done()
    };
    let whole = |line: Line<'static>| {
        let mut cells = columns.row();
        cells.span(line, 2);
        cells.done()
    };
    let mut rows = Vec::new();
    for (index, row) in AudioRow::ALL.into_iter().enumerate() {
        let here = index == app.audio_row;
        let line = match row {
            AudioRow::Roto | AudioRow::Wheel => switch_value(app, row, here),
            _ => Line::styled(value(app, row), setting::style(app, here, false)),
        };
        rows.push(named(row.name(), line));
    }
    rows.push(blank_row());
    let audio = &app.audio;
    if audio.running {
        // The device's own delay is in this too, which on Bluetooth is most of it.
        let latency = audio.latency_us as f64 / 1000.0;
        rows.push(named(
            "Latency",
            Line::raw(format!("{latency:.1} ms, out of the device")),
        ));
        let trouble = audio.late > 0 || audio.xruns > 0;
        let figures = Span::styled(
            format!(
                "CPU {}%  late {}  xruns {}",
                app.cpu.percent(),
                audio.late,
                audio.xruns
            ),
            if trouble {
                theme::warning()
            } else {
                Style::new()
            },
        );
        // The status line says what the leaf means.
        let line: Line = system::leaf(app.slowdown.slowed())
            .into_iter()
            .chain([figures])
            .collect();
        rows.push(named("Keeping up", line));
    } else {
        rows.push(whole(Line::styled(
            "Stopped. Choose a device to play again.",
            theme::warning(),
        )));
    }
    rows.push(blank_row());
    rows.push(whole(Line::styled(
        "A smaller buffer answers keys sooner, but drops out sooner when the CPU is busy.",
        theme::faint(),
    )));
    columns.stack(frame, &mut rest, rows);
}
