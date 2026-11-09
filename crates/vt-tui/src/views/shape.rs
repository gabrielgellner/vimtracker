//! The shape panel, over the right of the screen so the column it fills stays in view.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use vt_model::shape::{FillKind, ShapeColumn, Where, values};

use crate::app::App;
use crate::input::mode::Mode;
use crate::shaping::{ShapePanel, ShapeRow};
use crate::theme;

const WIDTH: u16 = 38;
/// The border, the heading and the keys, beyond the rows.
const AROUND_ROWS: u16 = 4;
/// Clear of the CPU figure at the top of the screen.
const FROM_TOP: u16 = 2;

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
        ShapeRow::From => format!("{:02X}", shape.from),
        ShapeRow::To => format!("{:02X}", shape.to),
        ShapeRow::Curve | ShapeRow::S if shape.kind == FillKind::Random => "--".to_owned(),
        ShapeRow::Curve => bend(shape.curve),
        ShapeRow::S => bend(shape.s),
        ShapeRow::Fill => shape.kind.name().to_owned(),
        ShapeRow::Wave => shape.wave.name().to_owned(),
        ShapeRow::Rate => shape.rate.to_string(),
        ShapeRow::Phase => format!("{}°", shape.phase),
        ShapeRow::Values => shape.values.to_string(),
        ShapeRow::Run => shape.run.to_string(),
        ShapeRow::Where => shape.place.name().to_owned(),
        ShapeRow::Amount => match shape.place {
            Where::Every(every) => format!("{every} rows"),
            Where::Chance(percent) => format!("{percent}%"),
            _ => String::new(),
        },
    }
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
        ShapeColumn::Velocity => "Velocity".to_owned(),
        ShapeColumn::Fx { command, .. } => app
            .song()
            .tracks
            .get(panel.track)
            .map_or_else(|| command.name().to_owned(), |track| command.name_on(track)),
    };
    let range = panel.selection.range;
    let filled = values(panel.base_steps(), range, panel.column, &panel.shape).len();
    let mut lines = vec![Line::styled(
        format!("{name}: {filled} of {} rows", range.len()),
        theme::dim(),
    )];
    for row in rows {
        let current = row == panel.row;
        let typing = current.then_some(panel.typing.as_ref()).flatten();
        let text = typing.map_or_else(|| value(panel, row), |text| format!("{text}_"));
        let style = if current {
            theme::cursor(if panel.typing.is_some() {
                Mode::Insert
            } else {
                Mode::Normal
            })
        } else {
            Style::new()
        };
        lines.push(Line::from(vec![
            Span::raw(format!("{:<8}", row.label())),
            Span::styled(text, style),
        ]));
    }
    let hint = match panel.shape.kind {
        FillKind::Random if app.mode == Mode::Shape => "~ swap  r reroll",
        FillKind::List => "~ reverse the list",
        _ => "~ swap the ends",
    };
    lines.push(Line::styled(hint, theme::dim()));
    frame.render_widget(Clear, window);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::new()
                .style(theme::window())
                .borders(Borders::ALL)
                .title(" Shape ")
                .title_bottom(" Enter keep  Esc cancel ")
                .title_style(Style::new().add_modifier(Modifier::BOLD)),
        ),
        window,
    );
}
