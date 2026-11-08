//! The audio page: the output playing, its rate and buffer, and how it's keeping up.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::App;
use crate::audio_page::{AudioRow, buffer_ms};
use crate::theme;

/// Room for a row's name.
const NAME_WIDTH: usize = 12;

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
    }
}

pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let bold = Style::new().add_modifier(Modifier::BOLD);
    let mut lines = vec![Line::styled("Audio", bold), Line::raw("")];
    for (index, row) in AudioRow::ALL.into_iter().enumerate() {
        let style = if index == app.audio_row {
            theme::cursor(app.mode)
        } else {
            Style::new()
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{:<NAME_WIDTH$}", row.name()), theme::dim()),
            Span::styled(value(app, row), style),
        ]));
    }
    lines.push(Line::raw(""));
    let audio = &app.audio;
    if audio.running {
        // The device's own delay is in this too, which on Bluetooth is most of it.
        let latency = audio.latency_us as f64 / 1000.0;
        lines.push(Line::from(vec![
            Span::styled(format!("{:<NAME_WIDTH$}", "Latency"), theme::dim()),
            Span::raw(format!("{latency:.1} ms, out of the device")),
        ]));
        let trouble = audio.late > 0 || audio.xruns > 0;
        lines.push(Line::from(vec![
            Span::styled(format!("{:<NAME_WIDTH$}", "Keeping up"), theme::dim()),
            Span::styled(
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
            ),
        ]));
    } else {
        lines.push(Line::styled(
            "Stopped. Choose a device to play again.",
            theme::warning(),
        ));
    }
    lines.push(Line::raw(""));
    lines.push(Line::styled(
        "A smaller buffer answers keys sooner, but drops out sooner when the CPU is busy.",
        theme::faint(),
    ));
    frame.render_widget(Paragraph::new(lines), area);
}
