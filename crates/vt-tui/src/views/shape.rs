//! The shape panel, over the right of the screen so the column it fills stays in view.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};
use vt_model::lfo::Shape as Wave;
use vt_model::shape::{FillKind, ShapeColumn, Where, first_note, written_from};
use vt_model::step::Note;

use crate::app::App;
use crate::input::mode::Mode;
use crate::shaping::{ShapePanel, ShapeRow};
use crate::theme;
use crate::views::widgets;

/// Room for eight notes in a `List`'s values.
const WIDTH: u16 = 42;
/// The border, the heading and the keys, beyond the rows.
const AROUND_ROWS: u16 = 4;
/// Clear of the CPU figure at the top of the screen.
const FROM_TOP: u16 = 2;
/// The room a row's label takes unless one is longer.
const LABEL: usize = 8;

/// A bend, with its sign.
fn bend(value: i8) -> String {
    if value > 0 {
        format!("+{value}")
    } else {
        value.to_string()
    }
}

/// A row's value as text.
fn value(panel: &ShapePanel, row: ShapeRow) -> String {
    let shape = &panel.shape;
    match row {
        ShapeRow::From => panel.column.show(shape.from),
        ShapeRow::To => panel.column.show(shape.to),
        ShapeRow::Curve | ShapeRow::S if shape.kind == FillKind::Random => "--".to_owned(),
        ShapeRow::Curve => bend(shape.curve),
        ShapeRow::S => bend(shape.s),
        ShapeRow::Fill => shape.kind.name().to_owned(),
        ShapeRow::Wave => shape.wave.name().to_owned(),
        ShapeRow::Rate => shape.rate.to_string(),
        ShapeRow::Phase => format!("{}°", shape.phase),
        ShapeRow::Values => panel.column.show_list(&shape.values),
        ShapeRow::Run => shape.run.to_string(),
        ShapeRow::Where => shape.place.name().to_owned(),
        ShapeRow::Amount => match shape.place {
            Where::Every(every) => format!("{every} rows"),
            Where::Chance(percent) => format!("{percent}%"),
            _ => String::new(),
        },
        ShapeRow::Hits => match shape.place {
            Where::Euclid { hits, .. } => {
                let open = panel.open_rows();
                format!("{} of {open}", usize::from(hits).min(open))
            }
            _ => String::new(),
        },
        ShapeRow::Rotate => match shape.place {
            Where::Euclid { rotate, .. } => rotate.to_string(),
            _ => String::new(),
        },
        ShapeRow::Notes => shape.acid.notes.to_string(),
        ShapeRow::Octave => format!("{}%", shape.acid.octave),
        ShapeRow::Density => format!("{}%", shape.acid.density),
        ShapeRow::Accent => format!("{}%", shape.acid.accent),
        ShapeRow::Slide => format!("{}%", shape.acid.slide),
        ShapeRow::Pitches if shape.acid.keep => "Keep".to_owned(),
        ShapeRow::Pitches => "Roll".to_owned(),
        ShapeRow::Source => shape.follow.source.to_string(),
        ShapeRow::Transpose => {
            let by = shape.follow.transpose;
            let unit = match panel.column {
                ShapeColumn::Note { scale, .. } if scale.on => "degree",
                _ => "semitone",
            };
            match by {
                0 => "0".to_owned(),
                1 | -1 => format!("{by:+} {unit}"),
                _ => format!("{by:+} {unit}s"),
            }
        }
        ShapeRow::Invert => on_off(shape.follow.invert),
        ShapeRow::Axis => match shape.follow.axis {
            Some(note) => panel.column.show(note),
            None => match panel.source_steps().and_then(first_note) {
                Some(note) => format!("First, {}", panel.column.show(note)),
                None => "First".to_owned(),
            },
        },
        ShapeRow::Reverse => on_off(shape.follow.reverse),
        ShapeRow::Late => match shape.follow.rotate {
            0 => "0".to_owned(),
            1 => "1 row late".to_owned(),
            rows => format!("{rows} rows late"),
        },
        ShapeRow::CopyFx => on_off(shape.follow.fx),
    }
}

fn on_off(on: bool) -> String {
    if on { "On" } else { "Off" }.to_owned()
}

/// A row's value in spans, with a note outside the scale coloured as the grid colours it.
fn spans(panel: &ShapePanel, row: ShapeRow) -> Vec<Span<'static>> {
    let ShapeColumn::Note { scale, .. } = panel.column else {
        return vec![Span::raw(value(panel, row))];
    };
    let notes: Vec<u8> = match row {
        ShapeRow::From => vec![panel.shape.from],
        ShapeRow::To => vec![panel.shape.to],
        ShapeRow::Values => panel.shape.values.as_slice().to_vec(),
        _ => return vec![Span::raw(value(panel, row))],
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
    let source = panel.source_steps();
    for (nth, part) in panel.parts.iter().enumerate() {
        let steps = panel.base_steps(part);
        let shape = panel.shape_of(nth);
        let written = written_from(steps, part.range, part.column, &shape, source);
        // A `Follow` writes every row, rests and all, so its count is of the notes.
        filled += if shape.kind == FillKind::Follow {
            written
                .iter()
                .filter(|(_, step)| matches!(step.note, Note::On(_)))
                .count()
        } else {
            written.len()
        };
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
    // Room for the longest label, as `Transpose` is longer than the rest.
    let labels = rows
        .iter()
        .map(|row| row.label().len() + 1)
        .max()
        .unwrap_or(0)
        .max(LABEL);
    for row in rows {
        let current = row == panel.row;
        let typing = current.then_some(panel.typing.as_ref()).flatten();
        let mut line = vec![Span::raw(format!("{:<labels$}", row.label()))];
        if current {
            let text = typing.map_or_else(|| value(panel, row), |text| format!("{text}_"));
            let style = theme::cursor(if panel.typing.is_some() {
                Mode::Insert
            } else {
                Mode::Normal
            });
            line.push(Span::styled(text, style));
        } else {
            line.extend(spans(panel, row));
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
        FillKind::Follow => "L keeps it in step with its source",
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
