//! The status line: mode, transport, tempo, load and messages.

use std::sync::atomic::Ordering;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use vt_engine::engine::Transport;

use crate::app::App;
use crate::audio::AudioStats;
use crate::input::keys::Key;
use crate::input::mode::Mode;

/// Show a tempo without trailing zeros.
pub fn format_bpm(millibpm: u32) -> String {
    let whole = millibpm / 1000;
    let fraction = millibpm % 1000;
    if fraction == 0 {
        whole.to_string()
    } else {
        format!("{whole}.{fraction:03}")
            .trim_end_matches('0')
            .to_owned()
    }
}

/// Draw the status line and the message or command line below it.
pub fn draw(frame: &mut Frame, area: Rect, app: &App, stats: Option<&AudioStats>) {
    let transport = match app.transport {
        Transport::Stopped => "stopped",
        Transport::Playing => "playing",
        Transport::Paused => "paused",
    };
    let (load, late, errors) = stats.map_or((0, 0, 0), |s| {
        (
            s.load.load(Ordering::Relaxed),
            s.late.load(Ordering::Relaxed),
            s.errors.load(Ordering::Relaxed),
        )
    });
    let mode_style = Style::new()
        .fg(Color::Black)
        .bg(match app.mode {
            Mode::Normal => Color::Blue,
            Mode::Insert => Color::Green,
            Mode::Command => Color::Yellow,
        })
        .add_modifier(Modifier::BOLD);
    let pending = if app.pending().is_empty() {
        String::new()
    } else {
        let keys: Vec<String> = app.pending().iter().map(Key::to_string).collect();
        format!("  {}", keys.join(" "))
    };
    let dirty = if app.dirty { " *" } else { "" };
    let status = Line::from(vec![
        Span::styled(format!(" {} ", app.mode.label()), mode_style),
        Span::raw(format!(
            " {transport}  {} BPM  CPU {:.1}%  late {late}  xruns {errors}{dirty}{pending}",
            format_bpm(app.song().millibpm),
            load as f64 / 10.0,
        )),
    ]);
    let message = if app.mode == Mode::Command {
        format!(":{}", app.command_line)
    } else {
        app.status.clone()
    };
    frame.render_widget(Paragraph::new(vec![status, Line::raw(message)]), area);
}

#[cfg(test)]
mod tests {
    use super::format_bpm;

    #[test]
    fn tempos_drop_trailing_zeros() {
        assert_eq!(format_bpm(120_000), "120");
        assert_eq!(format_bpm(137_630), "137.63");
        assert_eq!(format_bpm(99_001), "99.001");
    }
}
