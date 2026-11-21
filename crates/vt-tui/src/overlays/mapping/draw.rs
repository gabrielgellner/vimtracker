//! A macro's mapping view, after Live's mapping browser: a window over the track page listing each
//! setting the macro holds, with the ends of its range, `Min` at the start of its zone and `Max`
//! at its end, its response's `Curve` and `S`, and its `Zone`, the part of the macro's turn it
//! acts over, `00–FF` for all of it, the cursor on one of them, and beside them the response of
//! the mapping under the cursor, drawn.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use vt_model::macros::MacroMap;

use crate::controls::macro_name;
use crate::graphics::PanelKey;
use crate::input::typed::TypedValue;
use crate::overlays::mapping::{MapField, MappingView, bend_text, end_text, map_name};
use crate::pages::Ctx;
use crate::widgets::theme;
use crate::widgets::{image, popup};

/// How wide the setting's column is, each end's, and each bend's.
const SETTING_WIDTH: usize = 26;
const END_WIDTH: usize = 7;
const BEND_WIDTH: usize = 6;
/// Each end of the zone, two hex digits and the dash or the space after it.
const ZONE_END_WIDTH: usize = 3;
/// Room after the bends for `by hand`.
const BY_HAND_WIDTH: usize = 8;
/// The response's drawing, in characters, inside its box: square at the author's 15 by 39
/// pixels, as the compressor's curve is.
pub(crate) const RESPONSE_WIDTH: usize = 13;
pub(crate) const RESPONSE_HEIGHT: usize = 5;
/// The room between the table and the drawing's box.
const RESPONSE_GAP: usize = 2;
/// The keys it takes, along its bottom.
const KEYS: &str = " = - move  i types  x straight or whole  ~ swaps  dd unmaps  Esc ";

/// How wide each column of a mapping's row is, by what's in it.
fn width_of(field: MapField) -> usize {
    if field.zones() {
        ZONE_END_WIDTH
    } else if field.bends() {
        BEND_WIDTH
    } else {
        END_WIDTH
    }
}

/// Draw the mapping view in the middle of an area.
pub fn draw(frame: &mut Frame, area: Rect, ctx: &Ctx, view: &MappingView) {
    let Some(track) = ctx.song().tracks.get(ctx.cursor().track) else {
        return;
    };
    let held = &track.macros[view.at];
    let table =
        2 + SETTING_WIDTH + 2 * END_WIDTH + 2 * BEND_WIDTH + 2 * ZONE_END_WIDTH + BY_HAND_WIDTH;
    let boxed = RESPONSE_WIDTH + 2;
    let width = (2 + table + RESPONSE_GAP + boxed + 1).max(KEYS.len() + 4) as u16;
    // A line above the drawing's box and one below it.
    let height = (held.maps.len() + 5).max(RESPONSE_HEIGHT + 2 + 4) as u16;
    let window = popup::centred(area, width, height);
    let count = match held.maps.len() {
        1 => "1 mapping".to_owned(),
        count => format!("{count} mappings"),
    };
    let title = Line::from(vec![
        Span::styled(
            format!(" Macro {} {} ", view.at + 1, macro_name(held, view.at)),
            Style::new().add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!(" {count} "), theme::dim()),
    ]);
    let border = popup::border()
        .title(title)
        .title_bottom(Line::styled(KEYS, theme::dim()));
    let inner = popup::draw(frame, window, border);
    let mut lines = vec![
        Line::raw(""),
        Line::styled(heading(), theme::dim()),
        Line::raw(""),
    ];
    for (row, map) in held.maps.iter().enumerate() {
        let here = (row == view.row).then_some((view.field, view.typing.as_ref()));
        lines.push(mapping_row(ctx, map, row == view.row, here));
    }
    frame.render_widget(Paragraph::new(lines), inner);
    // The response of the mapping under the cursor, its box drawn empty until the terminal has
    // its picture.
    let response = Rect {
        x: inner.x + (table + RESPONSE_GAP) as u16,
        y: inner.y + 1,
        width: boxed as u16,
        height: (RESPONSE_HEIGHT + 2) as u16,
    }
    .intersection(inner);
    let drawing = match view.images.get(PanelKey::Mapping) {
        Some(id) => image::boxed(id, RESPONSE_WIDTH, RESPONSE_HEIGHT),
        None => empty_box(RESPONSE_WIDTH, RESPONSE_HEIGHT),
    };
    frame.render_widget(Paragraph::new(drawing), response);
}

/// The mappings' table's heading, each column's name over it, as the view and the Macros page
/// (decision 465) show it.
pub(crate) fn heading() -> String {
    let mut heading = format!("  {:<SETTING_WIDTH$}", "Setting");
    for field in MapField::ALL {
        // The zone's two ends share a heading.
        let width = match field {
            MapField::ZoneStart => 2 * ZONE_END_WIDTH,
            MapField::ZoneEnd => continue,
            _ => width_of(field),
        };
        heading.push_str(&format!("{:<width$}", field.name()));
    }
    heading
}

/// A mapping's row of the table: its setting, the ends of its range, its bends and its zone.
/// `marked` puts the view's `›` before it, and `here` is the field the view's cursor is on in it,
/// with what's typed there so far.
pub(crate) fn mapping_row(
    ctx: &Ctx,
    map: &MacroMap,
    marked: bool,
    here: Option<(MapField, Option<&TypedValue>)>,
) -> Line<'static> {
    let target = map.target;
    let marker = if marked { "›" } else { " " };
    let name: String = map_name(ctx.song(), ctx.cursor().track, map)
        .chars()
        .take(SETTING_WIDTH - 1)
        .collect();
    // A generator's `Wave` or `Rate` is reached by hand alone, which its row says, dim.
    let by_hand = target.by_hand();
    let cell = |field: MapField| {
        let on = here.is_some_and(|(at, _)| at == field);
        let text = match here {
            Some((_, Some(typed))) if on => typed.shown(),
            _ => match field {
                MapField::Min => end_text(target, map.min),
                MapField::Max => end_text(target, map.max),
                MapField::Curve => bend_text(map.curve),
                MapField::S => bend_text(map.s),
                MapField::ZoneStart => format!("{:02X}", map.zone.start),
                MapField::ZoneEnd => format!("{:02X}", map.zone.end),
            },
        };
        let style = if on {
            theme::cursor(ctx.mode())
        } else if by_hand
            || (field.bends() && map.is_straight())
            || (field.zones() && map.zone.is_whole())
        {
            // A straight response and the whole turn are dim, so a bent one or a zone stands
            // out.
            theme::dim()
        } else {
            Style::new()
        };
        let padded = format!("{text:<width$}", width = width_of(field) - 1);
        // The zone reads as one, `00–7F`.
        let after = if field == MapField::ZoneStart {
            Span::styled("–", theme::dim())
        } else {
            Span::raw(" ")
        };
        [Span::styled(padded, style), after]
    };
    let name_style = if by_hand { theme::dim() } else { Style::new() };
    let mut spans = vec![
        Span::styled(format!(" {marker}"), theme::dim()),
        Span::styled(format!("{name:<SETTING_WIDTH$}"), name_style),
    ];
    for field in MapField::ALL {
        spans.extend(cell(field));
    }
    if by_hand {
        spans.push(Span::styled("by hand", theme::dim()));
    }
    Line::from(spans)
}

/// A faint box with nothing in it, as [`image::boxed`] draws one round a picture.
fn empty_box(width: usize, height: usize) -> Vec<Line<'static>> {
    let rule = "─".repeat(width);
    let edge = theme::faint();
    let mut lines = vec![Line::styled(format!("┌{rule}┐"), edge)];
    for _ in 0..height {
        lines.push(Line::styled(format!("│{}│", " ".repeat(width)), edge));
    }
    lines.push(Line::styled(format!("└{rule}┘"), edge));
    lines
}
