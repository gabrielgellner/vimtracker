//! The phrase view: one track's steps, the cursor and the playhead.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use vt_engine::engine::Transport;
use vt_model::constants::BEAT_STEPS;
use vt_model::step::Step;

use crate::app::{App, Field};

/// The character range of a field within a step's text.
fn field_range(step: &Step, field: Field) -> (usize, usize) {
    let instrument_width = step.instrument.map_or(2, |id| format!("{id:02X}").len());
    let fx_start = |slot: usize| 7 + instrument_width + 1 + slot * 6;
    match field {
        Field::Note => (0, 3),
        Field::Volume => (4, 6),
        Field::Instrument => (7, 7 + instrument_width),
        Field::FxCommand(slot) => (fx_start(slot), fx_start(slot) + 3),
        Field::FxValue(slot) => (fx_start(slot) + 3, fx_start(slot) + 5),
    }
}

/// Which step of this phrase the playhead is on, if it is on this phrase.
fn playhead_step(app: &App) -> Option<usize> {
    if app.transport != Transport::Playing && app.transport != Transport::Paused {
        return None;
    }
    let song = app.song();
    let chain = (*song.sections.first()?.chains.get(app.cursor.track)?)?;
    let position = song.chain_position(chain, app.playhead)?;
    (position.slot == app.cursor.slot && Some(position.phrase) == app.phrase_id())
        .then_some(position.step)
}

/// Draw the view.
pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let Some(phrase) = app.phrase_id().and_then(|id| app.song().phrases.get(&id)) else {
        frame.render_widget(Paragraph::new("This track has no phrase here."), area);
        return;
    };
    let height = usize::from(area.height.saturating_sub(1)).max(1);
    let top = app
        .cursor
        .step
        .saturating_sub(height / 2)
        .min(phrase.len().saturating_sub(height));
    let playhead = playhead_step(app);
    let mut lines = vec![Line::styled(
        format!(
            "Track {:02}  Phrase {:02X}  Octave {}  Instrument {:02X}",
            app.cursor.track + 1,
            app.phrase_id().unwrap_or(0),
            app.octave,
            app.instrument,
        ),
        Style::new().add_modifier(Modifier::BOLD),
    )];
    for (index, step) in phrase.steps().iter().enumerate().skip(top).take(height) {
        let text = step.to_string();
        let marker = if playhead == Some(index) { ">" } else { " " };
        let number = Style::new().fg(if index % BEAT_STEPS == 0 {
            Color::Yellow
        } else {
            Color::DarkGray
        });
        let mut spans = vec![
            Span::styled(format!("{index:02X}"), number),
            Span::styled(format!(" {marker} "), Style::new().fg(Color::Green)),
        ];
        let body = if playhead == Some(index) {
            Style::new().fg(Color::Green)
        } else {
            Style::new()
        };
        if index == app.cursor.step {
            let (start, end) = field_range(step, app.cursor.field);
            let end = end.min(text.len());
            let start = start.min(end);
            spans.push(Span::styled(text[..start].to_owned(), body));
            spans.push(Span::styled(
                text[start..end].to_owned(),
                Style::new().add_modifier(Modifier::REVERSED),
            ));
            spans.push(Span::styled(text[end..].to_owned(), body));
        } else {
            spans.push(Span::styled(text, body));
        }
        lines.push(Line::from(spans));
    }
    frame.render_widget(Paragraph::new(lines), area);
}
