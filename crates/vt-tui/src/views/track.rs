//! The track page: what a track holds apart from its steps, grouped by theme.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use vt_model::action::TrackControl;
use vt_model::lfo::depth_text;
use vt_model::song::Track;

use crate::app::App;
use crate::input::mode::Mode;
use crate::theme;
use crate::track_page::{Control, FILTER, LFOS, LfoField, OUTPUT};
use crate::views::status::format_swing;

/// Room for a section's name.
const NAME_WIDTH: usize = 10;

/// A setting's value as text.
fn value(control: Control, track: &Track, project_swing: u16) -> String {
    let hex = |value: u8| format!("{value:02X}");
    match control {
        Control::Base(TrackControl::Cutoff) => hex(track.cutoff),
        Control::Base(TrackControl::Resonance) => hex(track.resonance),
        Control::Base(TrackControl::Pan) => hex(track.pan),
        Control::Base(TrackControl::Delay) => hex(track.delay),
        Control::Base(TrackControl::Reverb) => hex(track.reverb),
        Control::Level => hex(track.level),
        Control::Swing => format_swing(track.swing.unwrap_or(project_swing)),
        Control::Lfo(slot, field) => {
            let lfo = &track.lfos[slot];
            match field {
                LfoField::Destination => lfo
                    .destination
                    .map_or_else(|| "---".to_owned(), |d| d.name().to_owned()),
                LfoField::Shape => lfo.shape.name().to_owned(),
                LfoField::Rate => lfo.rate.to_string(),
                LfoField::Depth => depth_text(lfo.depth),
                LfoField::Phase => format!("{}°", lfo.phase),
                LfoField::Retrig => if lfo.retrig { "on" } else { "off" }.to_owned(),
            }
        }
    }
}

/// How wide a setting's column is, so columns line up and don't move as values change.
fn width(control: Control) -> usize {
    match control {
        Control::Lfo(_, LfoField::Rate) => 9,
        Control::Lfo(_, LfoField::Shape) => 8,
        Control::Base(TrackControl::Reverb) | Control::Lfo(_, LfoField::Retrig) => 8,
        _ => 7,
    }
}

/// The column names over a row.
fn heading(controls: &[Control]) -> Line<'static> {
    let mut text = " ".repeat(NAME_WIDTH);
    for control in controls {
        text.push_str(&format!(
            "{:<width$}",
            control.label(),
            width = width(*control)
        ));
    }
    Line::styled(text, theme::dim())
}

/// A row of settings, with the cursor on one of them.
fn row(app: &App, track: &Track, name: &str, controls: &[Control]) -> Line<'static> {
    let mut spans = vec![Span::raw(format!("{name:<NAME_WIDTH$}"))];
    let current = app.track_control();
    for control in controls {
        let typing = *control == current && app.mode == Mode::Insert;
        let text = if typing {
            format!("{}_", app.entry.text)
        } else {
            value(*control, track, app.song().swing)
        };
        // A track following the project's swing shows what it plays, dimmed as it isn't its own.
        let style = if *control == current {
            theme::cursor(app.mode)
        } else if *control == Control::Swing && track.swing.is_none() {
            theme::dim()
        } else {
            Style::new()
        };
        let room = width(*control);
        let gap = room.saturating_sub(text.chars().count());
        spans.push(Span::styled(text, style));
        spans.push(Span::raw(" ".repeat(gap)));
    }
    Line::from(spans)
}

/// Draw the view.
pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let Some(track) = app.song().tracks.get(app.cursor.track) else {
        frame.render_widget(Paragraph::new("This track doesn't exist."), area);
        return;
    };
    let bold = Style::new().add_modifier(Modifier::BOLD);
    let mut lines = vec![
        Line::styled(format!("Track {:02}", app.cursor.track + 1), bold),
        Line::raw(""),
        heading(&FILTER),
        row(app, track, "Filter", &FILTER),
        Line::raw(""),
        heading(&OUTPUT),
        row(app, track, "Output", &OUTPUT),
        Line::raw(""),
        heading(&LFOS[0]),
    ];
    for (slot, controls) in LFOS.iter().enumerate() {
        lines.push(row(app, track, &format!("LFO {}", slot + 1), controls));
    }
    frame.render_widget(Paragraph::new(lines), area);
}
