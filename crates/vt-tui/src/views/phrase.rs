//! The phrase view: one track's steps, the cursor and the playhead.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use vt_engine::engine::Transport;
use vt_model::constants::BEAT_STEPS;
use vt_model::label::{is_named_slot, name_of};
use vt_model::selection::Scope;
use vt_model::step::Step;

use crate::app::{App, Field};
use crate::input::mode::Mode;
use crate::theme;

/// The width of a read-only note column for another track.
const NEIGHBOUR_WIDTH: usize = 4;
/// Room for the distance from the cursor, the step number, the playhead marker and a gap.
const GUTTER: usize = 8;

/// The rows that aren't steps: the heading, and the box's top and bottom. The box always has its
/// room, so nothing moves when Insert starts.
pub const ROWS_AROUND_STEPS: u16 = 3;

/// A step's text on screen, which shows a named slot by its name rather than its number.
fn step_text(step: &Step) -> String {
    let text = step.to_string();
    match step.instrument {
        Some(id) if is_named_slot(id) => {
            let number = format!("{id:02X}").len();
            format!("{}{}{}", &text[..7], name_of(id), &text[7 + number..])
        }
        _ => text,
    }
}

/// Split text into runs of the same style, given a style for each character.
fn spans_of(text: &str, styles: &[Style]) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut start = 0;
    for end in 1..=text.len() {
        if end == text.len() || styles[end] != styles[start] {
            spans.push(Span::styled(text[start..end].to_owned(), styles[start]));
            start = end;
        }
    }
    spans
}

/// The character range of a field within a step's text.
fn field_range(step: &Step, field: Field) -> (usize, usize) {
    let instrument_width = step.instrument.map_or(2, |id| name_of(id).len());
    let fx_start = |slot: usize| 7 + instrument_width + 1 + slot * 6;
    match field {
        Field::Note => (0, 3),
        Field::Velocity => (4, 6),
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

/// Whether another track is playing the step shown at a section position. A track shorter than
/// this one loops, so a step it is playing can show on more than one row.
fn neighbour_playing(app: &App, track: usize, section_step: usize) -> bool {
    if app.transport != Transport::Playing && app.transport != Transport::Paused {
        return false;
    }
    let song = app.song();
    let chain = song
        .sections
        .first()
        .and_then(|s| s.chains.get(track).copied().flatten());
    chain
        .and_then(|chain| song.chain_position(chain, section_step))
        .is_some_and(|shown| {
            chain.and_then(|chain| song.chain_position(chain, app.playhead)) == Some(shown)
        })
}

/// How far a row is from the cursor's, as the count to type before `j` or `k`, so it is decimal.
/// The cursor's own row is blank.
fn relative_number(index: usize, cursor: usize) -> String {
    match index.abs_diff(cursor) {
        0 => "   ".to_owned(),
        // Long phrases can reach three digits, which then use up the gap.
        distance => format!("{:<3}", format!("{distance:>2}")),
    }
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
    let height = usize::from(area.height.saturating_sub(ROWS_AROUND_STEPS)).max(1);
    let top = app.top;
    let selection = app.mode.is_visual().then(|| app.current_selection());
    let playhead = playhead_step(app);
    let body_width = phrase
        .steps()
        .iter()
        .map(|step| step_text(step).len())
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
    let cursor_style = theme::cursor(app.mode);
    let boxed = app.mode == Mode::Insert;
    let edge = Style::new().fg(theme::GREEN);
    let rule = |left: &str, right: &str| {
        if boxed {
            let inside = "─".repeat(body_width);
            let indent = " ".repeat(GUTTER - 1);
            Line::styled(format!("{indent}{left}{inside}{right}"), edge)
        } else {
            Line::raw("")
        }
    };
    let side = if boxed { "│" } else { " " };
    let mut lines = vec![
        Line::styled(title, Style::new().add_modifier(Modifier::BOLD)),
        rule("┌", "┐"),
    ];
    for (index, step) in phrase.steps().iter().enumerate().skip(top).take(height) {
        let text = step_text(step);
        let marker = if playhead == Some(index) { ">" } else { " " };
        let number = if index % BEAT_STEPS == 0 {
            Style::new().fg(theme::YELLOW)
        } else {
            Style::new().fg(theme::SUBTEXT1)
        };
        let mut spans = vec![
            Span::styled(relative_number(index, app.cursor.step), theme::faint()),
            Span::styled(format!("{index:02X}"), number),
            Span::styled(format!(" {marker}"), theme::active()),
            Span::styled(side, edge),
        ];
        let body = if playhead == Some(index) {
            theme::active()
        } else {
            Style::new()
        };
        let mut styles = vec![body; text.len()];
        if step.instrument.is_some_and(|id| app.song().is_labelled(id)) {
            let (start, end) = field_range(step, Field::Instrument);
            let label = body.patch(theme::label());
            styles[start..end.min(text.len())].fill(label);
        }
        let selected = selection.filter(|s| (s.range.first..=s.range.last).contains(&index));
        if let Some(selected) = selected {
            let (start, end) = selected_columns(step, selected.scope, text.len());
            styles[start..end].fill(theme::selected());
        }
        // The cursor stays visible inside a selection, so the moving end can be seen.
        if index == app.cursor.step {
            let (start, end) = field_range(step, app.cursor.field);
            let end = end.min(text.len());
            let start = start.min(end);
            let (first, last) = selected.map_or((start, end), |selected| {
                let (from, to) = selected_columns(step, selected.scope, text.len());
                (start.clamp(from, to), end.clamp(from, to))
            });
            styles[first..last].fill(cursor_style);
        }
        spans.extend(spans_of(&text, &styles));
        let padding = body_width.saturating_sub(text.len());
        spans.push(Span::raw(" ".repeat(padding)));
        spans.push(Span::styled(side, edge));
        spans.push(Span::raw(" "));
        for track in &others {
            let style = if neighbour_playing(app, *track, first_step + index) {
                theme::active()
            } else {
                theme::dim()
            };
            spans.push(Span::styled(
                format!("{} ", neighbour_note(app, *track, first_step + index)),
                style,
            ));
        }
        lines.push(Line::from(spans));
    }
    lines.push(rule("└", "┘"));
    frame.render_widget(Paragraph::new(lines), area);
}
