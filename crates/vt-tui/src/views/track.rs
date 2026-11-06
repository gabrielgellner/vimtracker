//! The track page: what a track holds apart from its steps, grouped by theme.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use vt_model::action::TrackControl;
use vt_model::constants::TRACK_ENVELOPES;
use vt_model::lfo::{Destination, depth_text};
use vt_model::song::Track;

use crate::app::App;
use crate::input::mode::Mode;
use crate::theme;
use crate::track_page::{Control, ENVELOPES, EnvelopeField, FILTER, LFOS, LfoField, OUTPUT};
use crate::views::braille::envelope_lines;
use crate::views::status::format_swing;

/// The size of an envelope's diagram, in characters.
const DIAGRAM_WIDTH: usize = 28;
const DIAGRAM_HEIGHT: usize = 4;

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
        Control::Envelope(slot, field) => {
            let envelope = &track.envelopes[slot];
            let ms = |value: u16| format!("{value} ms");
            match field {
                EnvelopeField::Destination => envelope
                    .destination
                    .map_or_else(|| "---".to_owned(), |d| d.name().to_owned()),
                EnvelopeField::Attack => ms(envelope.attack_ms),
                EnvelopeField::Decay => ms(envelope.decay_ms),
                EnvelopeField::Sustain => format!("{}%", envelope.sustain),
                EnvelopeField::Release => ms(envelope.release_ms),
                // The first envelope on `Vol` shapes each note, rather than moving a value.
                EnvelopeField::Depth
                    if slot == 0 && envelope.destination == Some(Destination::Vol) =>
                {
                    "--".to_owned()
                }
                EnvelopeField::Depth => depth_text(envelope.depth),
                EnvelopeField::Reset => if envelope.reset { "on" } else { "off" }.to_owned(),
            }
        }
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
        Control::Envelope(
            _,
            EnvelopeField::Attack
            | EnvelopeField::Decay
            | EnvelopeField::Sustain
            | EnvelopeField::Release,
        ) => 9,
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

/// Each envelope's shape, side by side, with the one being edited picked out.
fn diagrams(app: &App, track: &Track) -> Vec<Line<'static>> {
    let current = match app.track_control() {
        Control::Envelope(slot, _) => Some(slot),
        _ => None,
    };
    let shapes = track
        .envelopes
        .map(|envelope| envelope_lines(&envelope, DIAGRAM_WIDTH, DIAGRAM_HEIGHT));
    let style = |slot: usize| {
        if current == Some(slot) {
            theme::active()
        } else {
            theme::dim()
        }
    };
    let mut lines = vec![Line::from(
        (0..TRACK_ENVELOPES)
            .map(|slot| {
                let name = track.envelopes[slot]
                    .destination
                    .map_or("---", |destination| destination.name());
                let label = format!("Env {}  {name}", slot + 1);
                Span::styled(
                    format!("{label:<width$}", width = DIAGRAM_WIDTH + 2),
                    style(slot),
                )
            })
            .collect::<Vec<_>>(),
    )];
    lines.extend((0..DIAGRAM_HEIGHT).map(|row| {
        Line::from(
            (0..TRACK_ENVELOPES)
                .map(|slot| Span::styled(format!("{}  ", shapes[slot][row]), style(slot)))
                .collect::<Vec<_>>(),
        )
    }));
    lines
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
        heading(&ENVELOPES[0]),
    ];
    for (slot, controls) in ENVELOPES.iter().enumerate() {
        lines.push(row(app, track, &format!("Env {}", slot + 1), controls));
    }
    lines.push(Line::raw(""));
    lines.push(heading(&LFOS[0]));
    for (slot, controls) in LFOS.iter().enumerate() {
        lines.push(row(app, track, &format!("LFO {}", slot + 1), controls));
    }
    lines.push(Line::raw(""));
    lines.extend(diagrams(app, track));
    frame.render_widget(Paragraph::new(lines), area);
}
