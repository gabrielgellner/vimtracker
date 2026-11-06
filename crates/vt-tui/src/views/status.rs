//! The status line: mode, transport, tempo and messages.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use vt_engine::engine::Transport;
use vt_model::song::Machine;

use crate::app::{App, Field};
use crate::input::keys::Key;
use crate::input::mode::{Mode, View};
use crate::song_view::{COLUMNS_BEFORE_TRACKS, LENGTH_COLUMN, REPEAT_COLUMN, START_COLUMN};
use crate::theme;
use crate::tree::ChainColumn;

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

/// Show a swing in tenths of a percent without a trailing zero.
pub fn format_swing(swing: u16) -> String {
    if swing.is_multiple_of(10) {
        (swing / 10).to_string()
    } else {
        format!("{}.{}", swing / 10, swing % 10)
    }
}

/// The name of what the cursor is on, for the status line.
pub fn column_name(app: &App) -> String {
    match app.view {
        View::Phrase => match app.cursor.field {
            Field::Note => "Note".to_owned(),
            Field::Velocity => "Velocity".to_owned(),
            Field::FxCommand(slot) => format!("FX{} command", slot + 1),
            Field::FxValue(slot) => format!("FX{} value", slot + 1),
        },
        View::Chain => match app.chain_column {
            ChainColumn::Phrase => "Phrase".to_owned(),
            ChainColumn::Transpose => "Transpose".to_owned(),
        },
        View::Track | View::Mixer => {
            let machine = app
                .song()
                .tracks
                .get(app.cursor.track)
                .map_or(Machine::Sample, |track| track.machine);
            app.track_control().label_on(machine).to_owned()
        }
        View::Song => match app.song_column {
            LENGTH_COLUMN => "Length".to_owned(),
            REPEAT_COLUMN => "Rpt".to_owned(),
            START_COLUMN => "Start".to_owned(),
            column => {
                let track = column - COLUMNS_BEFORE_TRACKS + 1;
                let label = app.song_label();
                if label.is_empty() {
                    format!("Track {track:02} chain")
                } else {
                    format!("Track {track:02} chain: {label}")
                }
            }
        },
    }
}

/// Draw the status line and the message or command line below it.
pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let transport = match app.transport {
        Transport::Stopped => "stopped",
        Transport::Playing => "playing",
        Transport::Paused => "paused",
    };
    let mode_style = Style::new()
        .fg(theme::BASE)
        .bg(match app.mode {
            Mode::Normal
            | Mode::OperatorPending
            | Mode::Chain
            | Mode::Track
            | Mode::Mixer
            | Mode::Song => theme::BLUE,
            Mode::VisualField | Mode::VisualStep => theme::MAUVE,
            Mode::Insert => theme::GREEN,
            Mode::Command => theme::PEACH,
            Mode::Help | Mode::Guide | Mode::Picker | Mode::Shape => theme::TEAL,
            Mode::Replace | Mode::Restore => theme::RED,
        })
        .add_modifier(Modifier::BOLD);
    let pending = if app.pending().is_empty() {
        String::new()
    } else {
        let keys: Vec<String> = app.pending().iter().map(Key::to_string).collect();
        format!("  {}", keys.join(" "))
    };
    let dirty = if app.dirty { " *" } else { "" };
    let typed = if app.entry.text.is_empty() {
        String::new()
    } else {
        format!("  typed {}", app.entry.text)
    };
    // The cursor's track shows its own swing, marked so it isn't mistaken for the project's.
    let swing = match app
        .song()
        .tracks
        .get(app.cursor.track)
        .and_then(|t| t.swing)
    {
        Some(own) => format!("{} (track)", format_swing(own)),
        None => format_swing(app.song().swing),
    };
    let level = app
        .song()
        .tracks
        .get(app.cursor.track)
        .map_or(0, |t| t.level);
    // Only shown when on, since it changes what you hear and tracks otherwise run freely.
    let reset = app
        .section_here()
        .and_then(|s| s.reset)
        .map_or_else(String::new, |steps| format!("Reset {steps}  "));
    let sections = app.song().sections.len();
    let section = if sections > 1 {
        let looping = if app.looping.is_some() { " loop" } else { "" };
        format!("Sec {}/{sections}{looping}  ", app.cursor.section + 1)
    } else {
        String::new()
    };
    let reset = format!("{section}{reset}");
    let status = Line::from(vec![
        Span::styled(format!(" {} ", app.key_scope().label()), mode_style),
        Span::styled(format!(" {} ", column_name(app)), theme::selected()),
        Span::raw(format!(
            " {transport}  {} BPM  Swing {swing}  {reset}Oct {} Vel {:02X}  TVol {level:02X}{dirty}{pending}{typed}",
            format_bpm(app.song().millibpm),
            app.octave,
            app.note_velocity,
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
