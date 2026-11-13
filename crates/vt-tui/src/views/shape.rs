//! The shape panel, over the right of the screen so the column it fills stays in view.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};
use vt_model::lfo::Shape as Wave;
use vt_model::shape::{Context, FillKind, GenParam, ShapeColumn, written};

use crate::app::App;
use crate::input::mode::Mode;
use crate::shaping::ShapePanel;
use crate::theme;
use crate::views::widgets;

/// Room for eight notes in a `List`'s values.
const WIDTH: u16 = 42;
/// The border, the heading and the keys, beyond the rows.
const AROUND_ROWS: u16 = 4;
/// Clear of the CPU figure at the top of the screen.
const FROM_TOP: u16 = 2;

/// A row's value in spans, with a note outside the scale coloured as the grid colours it.
fn spans(panel: &ShapePanel, context: &Context, row: GenParam) -> Vec<Span<'static>> {
    let value = || vec![Span::raw(row.shown(&panel.shape, context))];
    let ShapeColumn::Note { scale, .. } = panel.column else {
        return value();
    };
    let notes: Vec<u8> = match row {
        GenParam::From => vec![panel.shape.from],
        GenParam::To => vec![panel.shape.to],
        GenParam::Values => panel.shape.values.as_slice().to_vec(),
        _ => return value(),
    };
    let mut spans = Vec::new();
    for (index, note) in notes.into_iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw(" "));
        }
        let style = if scale.on && !scale.contains(note) {
            theme::out_of_scale()
        } else {
            Style::new()
        };
        spans.push(Span::styled(panel.column.show(note), style));
    }
    spans
}

/// Draw the panel.
pub fn draw(frame: &mut Frame, area: Rect, app: &App, panel: &ShapePanel) {
    let rows = panel.rows();
    let context = panel.context();
    let width = WIDTH.min(area.width);
    let height = (rows.len() as u16 + AROUND_ROWS).min(area.height.saturating_sub(FROM_TOP));
    let window = Rect {
        x: area.x + area.width - width,
        y: area.y + FROM_TOP.min(area.height),
        width,
        height,
    };
    let name = match panel.column {
        ShapeColumn::Note { .. } => "Note".to_owned(),
        ShapeColumn::Velocity => "Velocity".to_owned(),
        ShapeColumn::Nudge => "Nudge".to_owned(),
        ShapeColumn::Length => "Length".to_owned(),
        ShapeColumn::Fx { command, .. } => app
            .song()
            .tracks
            .get(panel.parts[0].track)
            .map_or_else(|| command.name().to_owned(), |track| command.name_on(track)),
    };
    let (mut filled, mut rows_shaped) = (0, 0);
    for (nth, part) in panel.parts.iter().enumerate() {
        let steps = panel.base_steps(part);
        filled += written(steps, part.range, part.column, &panel.shape_of(nth)).len();
        rows_shaped += part.range.len();
    }
    let tracks = match panel.parts.len() {
        1 => String::new(),
        count => format!(" on {count} tracks"),
    };
    let live = if panel.live.is_some() { ", live" } else { "" };
    let mut lines = vec![Line::styled(
        format!("{name}{tracks}{live}: {filled} of {rows_shaped} rows"),
        theme::dim(),
    )];
    for row in rows {
        let current = row == panel.row;
        let typing = current.then_some(panel.typing.as_ref()).flatten();
        let mut line = vec![Span::raw(format!("{:<8}", row.name()))];
        if current {
            let text = typing.map_or_else(
                || row.shown(&panel.shape, &context),
                |text| format!("{text}_"),
            );
            let style = theme::cursor(if panel.typing.is_some() {
                Mode::Insert
            } else {
                Mode::Normal
            });
            line.push(Span::styled(text, style));
        } else {
            line.extend(spans(panel, &context, row));
        }
        lines.push(Line::from(line));
    }
    let hint = match panel.shape.kind {
        FillKind::Random if app.mode == Mode::Shape => "~ swap  r reroll",
        FillKind::Wave if panel.shape.wave == Wave::Smooth && app.mode == Mode::Shape => {
            "~ swap  r reroll"
        }
        FillKind::Shuffle if app.mode == Mode::Shape => "r deals again",
        FillKind::Acid if panel.shape.acid.keep => "r rolls the accents, slides and octaves",
        FillKind::Acid => "r rolls a new riff  ~ swap the ends",
        FillKind::List => "~ reverse the list",
        FillKind::Snap => match panel.column {
            ShapeColumn::Note { scale, .. } if scale.on => "To the nearest note in the scale",
            _ => "The scale is off, so nothing moves",
        },
        FillKind::Invert | FillKind::Shuffle | FillKind::Reverse => "From the values there",
        _ => "~ swap the ends",
    };
    lines.push(Line::styled(hint, theme::dim()));
    frame.render_widget(Clear, window);
    frame.render_widget(
        Paragraph::new(lines).block(
            widgets::window_border()
                .style(theme::window())
                // What it does: generate on an empty column, or reshape what's there.
                .title(format!(" {} ", panel.verb()))
                .title_bottom(if panel.live.is_some() {
                    " Enter apply  L keep live  Esc cancel "
                } else {
                    " Enter keep  L keep live  Esc cancel "
                })
                .title_style(Style::new().add_modifier(Modifier::BOLD)),
        ),
        window,
    );
}
