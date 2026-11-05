//! The phrase view: one track's steps, the cursor and the playhead.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use vt_engine::engine::Transport;
use vt_model::constants::BEAT_STEPS;
use vt_model::selection::Scope;
use vt_model::step::Step;

use crate::app::{App, Field};

/// The width of a read-only note column for another track.
const NEIGHBOUR_WIDTH: usize = 4;
/// Room for the step number, playhead marker and gap.
const GUTTER: usize = 5;

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

/// The character range a selection covers on one step.
fn selected_columns(step: &Step, scope: Scope, length: usize) -> (usize, usize) {
    if scope.is_whole() {
        return (0, length);
    }
    let slots = step.fx.len();
    let last = scope.last.min(2 + slots);
    let (start, _) = field_range(step, Field::of_group(scope.first.min(last)));
    let end = match Field::of_group(last) {
        Field::FxCommand(slot) => field_range(step, Field::FxValue(slot)).1,
        field => field_range(step, field).1,
    };
    (start.min(length), end.min(length))
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

/// Which other tracks fit beside the focused one, keeping those near it in view.
fn neighbours(app: &App, width: usize) -> Vec<usize> {
    let tracks = app.song().tracks.len();
    let columns = width / NEIGHBOUR_WIDTH;
    let others = tracks.saturating_sub(1);
    let shown = columns.min(others);
    if shown == 0 {
        return Vec::new();
    }
    let others: Vec<usize> = (0..tracks).filter(|t| *t != app.cursor.track).collect();
    let centre = others.partition_point(|t| *t < app.cursor.track);
    let start = centre.saturating_sub(shown / 2).min(others.len() - shown);
    others[start..start + shown].to_vec()
}

/// The note another track plays at a section position.
fn neighbour_note(app: &App, track: usize, section_step: usize) -> String {
    let song = app.song();
    song.sections
        .first()
        .and_then(|s| s.chains.get(track).copied().flatten())
        .and_then(|chain| song.chain_position(chain, section_step))
        .and_then(|position| song.step_at(position))
        .map_or_else(|| "   ".to_owned(), |step| step.note.to_string())
}

fn track_label(app: &App, track: usize) -> String {
    let live = app.levels.get(track).copied().unwrap_or_default();
    let flag = if live.mute {
        'm'
    } else if live.solo {
        's'
    } else {
        ' '
    };
    format!("{:02}{flag} ", track + 1)
}

/// Draw the view.
pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let Some(phrase) = app.phrase_id().and_then(|id| app.song().phrases.get(&id)) else {
        frame.render_widget(Paragraph::new("This track has no phrase here."), area);
        return;
    };
    let height = usize::from(area.height.saturating_sub(1)).max(1);
    let top = app.top;
    let selection = app.mode.is_visual().then(|| app.current_selection());
    let playhead = playhead_step(app);
    let body_width = phrase
        .steps()
        .iter()
        .map(|step| step.to_string().len())
        .max()
        .unwrap_or(0);
    let room = usize::from(area.width).saturating_sub(GUTTER + body_width + 2);
    let others = neighbours(app, room);
    let first_step = app.section_step() - app.cursor.step;
    let id = app.phrase_id().unwrap_or(0);
    let shared = if app.phrase_uses(id).len() > 1 {
        "*"
    } else {
        ""
    };
    let mut heading = format!(
        "Track {}Phrase {id:02X}{shared} {}/{}",
        track_label(app, app.cursor.track),
        app.cursor.slot + 1,
        app.chain_entries().len(),
    );
    heading.truncate(GUTTER + body_width);
    let mut title = format!("{heading:<width$}  ", width = GUTTER + body_width);
    for track in &others {
        title.push_str(&track_label(app, *track));
    }
    let mut lines = vec![Line::styled(
        title,
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
        let selected = selection.filter(|s| (s.range.first..=s.range.last).contains(&index));
        if let Some(selected) = selected {
            let (start, end) = selected_columns(step, selected.scope, text.len());
            let picked = Style::new().bg(Color::Blue).fg(Color::White);
            spans.push(Span::styled(text[..start].to_owned(), body));
            spans.push(Span::styled(text[start..end].to_owned(), picked));
            spans.push(Span::styled(text[end..].to_owned(), body));
        } else if index == app.cursor.step {
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
            spans.push(Span::styled(text.clone(), body));
        }
        let padding = body_width.saturating_sub(step.to_string().len()) + 2;
        spans.push(Span::raw(" ".repeat(padding)));
        for track in &others {
            spans.push(Span::styled(
                format!("{} ", neighbour_note(app, *track, first_step + index)),
                Style::new().fg(Color::DarkGray),
            ));
        }
        lines.push(Line::from(spans));
    }
    frame.render_widget(Paragraph::new(lines), area);
}
